//! Differential fuzzing of the JIT: a Lua program is built from the input bytes and run twice in
//! the same process, in a state with the interpreter only and in a state in `always` mode. The
//! `emit` log, the outcome class and the error text must be identical.
//!
//! First byte (config):
//!   bit 0   generational GC            bit 1   GC stress (one step per safepoint)
//!   bits 2-3  budget (grammar mode: tick limit 4000/500/60/7; raw mode: watchdog 300/150/80/40 ms)
//!   bit 4   grammar mode: also open the `debug` library (hooks, getinfo, traceback, getlocal)
//!   bits 5-6  tier 1.5 (typed regions) in the compiled run: 0 off, 1 `T15=always`, 2 `T15=on` with a warm-up
//!             profile, 3 `always` + `+verify`. When non-zero the NEXT byte configures it (see `T15Cfg::decode`): exit
//!             storm period (off/1/2/7/61), `+shadow`, `+randra` seed (docs/JIT_TIER15.md section 22)
//!   bit 7   raw source (the remaining bytes are the program) instead of the grammar of `gen.rs`
//!
//! Trace compiler (J5, needs `--features trace`; docs/JIT_TRACE.md 9.8): the first byte `0xFF` is an escape: the next
//! byte `t` configures the trace compiler of the compiled run, then the usual config byte (and T15 byte) follow. (The
//! config `0xFF` itself, raw + T15 verify + everything, is given up for it.) The compiled run then has T15 off.
//!   `t` bits 0-1  mode: 0 `TRACE=always`, 1 `on` hot 2,2, 2 `always`, 3 `on` hot 4,3
//!       bits 2-4  option variant: 0 default, 1 `+exitstorm=3`, 2 `+randra`, 3 `-fold,-cse,-loop,-fwd,-abc`, 4 `+verify`,
//!                 5 `+emurun`, 6 `+exitstorm=2,+randra`, 7 `+membudget`
//!       bit 5 `-loop` (rolled layout)   bit 6 `-fwd,-abc`   bit 7 `+tracemove`
//!   With the escape, bits 5-6 of the config byte (tier 1.5, unused: the compiled run has T15 off, and no T15 byte follows)
//!   mean (F6): bit 5 `-sink` (allocation sinking off; on otherwise, TR29), bit 6 reserved.
//!
//! The budget is cooperative in grammar mode: generated loops and functions call `tick(site)`, a
//! native that raises an uncatchable error after the limit, naming the site (so the point where the
//! budget runs out is compared exactly). The hook-based budget of `run_source` cannot be used: the
//! JIT is not entered while a hook is installed. Raw programs cannot carry ticks; they run under a
//! wall-clock watchdog (the interpreter's INTERRUPT flag) and runs it interrupted are not compared.
#![no_main]
use libfuzzer_sys::fuzz_target;
use rustynail_fuzz::{gen, run_diff, RunCfg, T15Cfg, TraceCfg};

fuzz_target!(|data: &[u8]| {
    if !rustynail_fuzz::HAS_JIT {
        panic!("build with --features jit");
    }
    let Some((&cfg0, rest0)) = data.split_first() else { return };
    // trace escape (see the module doc)
    let (trace, cfg, rest) = if cfg0 == 0xFF {
        let Some((&t, rest)) = rest0.split_first() else { return };
        let Some((&cfg, rest)) = rest.split_first() else { return };
        if !rustynail_fuzz::HAS_TRACE {
            panic!("build with --features trace");
        }
        let seed = u64::from(cfg) << 8 | u64::from(t);
        let tc = TraceCfg {
            on: t & 3 == 1 || t & 3 == 3,
            hot: if t & 3 == 3 { (4, 3) } else { (2, 2) },
            variant: (t >> 2) & 7,
            no_loop: t & 0x20 != 0,
            no_fwd: t & 0x40 != 0,
            tracemove: t & 0x80 != 0,
            no_sink: cfg & 0x20 != 0,
            seed: seed ^ rest.len() as u64,
        };
        (Some(tc), cfg, rest)
    } else {
        (None, cfg0, rest0)
    };
    // (the T15 byte is only present when bits 5-6 are set, so the byte layout of older seeds is unchanged)
    let (t15, rest) = match (cfg >> 5) & 3 {
        _ if trace.is_some() => (None, rest), // bits 5-6 are not the T15 selector under the trace escape
        0 => (None, rest),
        sel => {
            let Some((&b, rest)) = rest.split_first() else { return };
            (T15Cfg::decode(sel, b), rest)
        }
    };
    let raw = cfg & 0x80 != 0;
    let src = if raw {
        if rest.len() > 16384 {
            return;
        }
        rest.to_vec()
    } else {
        if rest.len() > 2048 {
            return;
        }
        gen::generate(rest, cfg & 0x10 != 0).into_bytes()
    };
    let dump = std::env::var_os("JIT_DIFF_DUMP").is_some();
    if dump {
        eprintln!("{}", String::from_utf8_lossy(&src));
    }
    let sel = ((cfg >> 2) & 3) as usize;
    // triage aid: JIT_DIFF_TICKS=n runs a (minimised) raw program with the same cooperative budget
    let env_ticks: Option<i64> = std::env::var("JIT_DIFF_TICKS").ok().and_then(|v| v.parse().ok());
    let mk = |jit: bool| RunCfg {
        gc: cfg & 3,
        jit,
        ticks: if raw { env_ticks } else { Some(env_ticks.unwrap_or([4000, 500, 60, 7][sel])) },
        // grammar programs are tick-bounded; the long deadline is a safety net (runs it interrupts are skipped)
        deadline_ms: if raw { Some([300, 150, 80, 40][sel]) } else { Some(5000) },
        debug_lib: !raw && cfg & 0x10 != 0,
        t15: if jit && trace.is_none() { t15 } else { None },
        trace: if jit { trace } else { None },
    };
    // triage aid: JIT_DIFF_ONLY=interp|jit runs one side only (to find out which one hangs)
    if let Ok(only) = std::env::var("JIT_DIFF_ONLY") {
        let r = run_diff(mk(only == "jit"), &src);
        eprintln!("only {only}: {:?}", r.as_ref().map(|r| (r.0.status, r.0.log.len(), r.1)));
        return;
    }
    let Some((a, ia)) = run_diff(mk(false), &src) else { return };
    let (b, ib) = run_diff(mk(true), &src).expect("jit run failed where the interpreter run succeeded");
    if ia || ib {
        return; // watchdog fired: not comparable
    }
    if let Some(dir) = std::env::var_os("JIT_DIFF_OUT") {
        // triage aid: src.lua, interp.log, jit.log in that directory
        let d = std::path::Path::new(&dir);
        let _ = std::fs::write(d.join("src.lua"), &src);
        let _ = std::fs::write(d.join("interp.log"), &a.log);
        let _ = std::fs::write(d.join("jit.log"), &b.log);
    }
    if dump {
        eprintln!("--- {:?}\n{}", a.status, String::from_utf8_lossy(&a.log));
    }
    if a != b {
        let s = String::from_utf8_lossy(&src);
        panic!(
            "DIVERGENCE (cfg {cfg:#x})\n--- source ---\n{s}\n--- interpreter ({:?}) ---\n{}\n--- jit ({:?}) ---\n{}",
            a.status,
            String::from_utf8_lossy(&a.log),
            b.status,
            String::from_utf8_lossy(&b.log)
        );
    }
});
