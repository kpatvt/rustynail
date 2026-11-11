//! Run arbitrary Lua source in a sandboxed state (pure libraries, instruction budget, memory cap,
//! collector configuration chosen by the first byte). Errors are fine; panics, hangs and memory
//! errors are bugs. With `--features jit` the state runs compiled code (`always` mode) when bit 7
//! of the first byte is set, so one corpus exercises both the interpreter and the JIT.
//! With `--features trace` (J5, docs/JIT_TRACE.md 9.8) bits 2-6 of the first byte configure the trace compiler of the
//! compiled run: bits 2-3 mode (0 off, 1 `TRACE=always`, 2 `on` hot 2,2, 3 `always` + `+tracemove`), bits 4-6 option
//! variant (`TraceCfg::opt`: 0 default, 1 `+exitstorm=3`, 2 `+randra`, 3 passes off, 4 `+verify`, 5 `+emurun`,
//! 6 `+exitstorm=2,+randra`, 7 `+membudget`). Bits 2-6 are ignored without bit 7 / without the feature.
//! F6: allocation sinking (TR29) is on in every variant but 3, which also turns it off (`-sink`), so one corpus covers
//! sunk tables, exit-time materialisation, virtual byte accounting and, with bit 1 (GC stress) and bit 0 (generational),
//! the recorder's watch list of dropped tables.
#![no_main]
use libfuzzer_sys::fuzz_target;
use rustynail_fuzz::{new_sandbox, run_budgeted, TraceCfg};

fuzz_target!(|data: &[u8]| {
    let Some((&cfg, src)) = data.split_first() else { return };
    if src.len() > 4096 {
        return;
    }
    rustynail_fuzz::jit_init();
    let Some(mut ls) = new_sandbox(cfg) else { return };
    let compiled = cfg & 0x80 != 0 && rustynail_fuzz::HAS_JIT;
    rustynail_fuzz::jit_mode(&mut ls, compiled);
    if compiled && rustynail_fuzz::HAS_TRACE && (cfg >> 2) & 3 != 0 {
        let m = (cfg >> 2) & 3;
        rustynail_fuzz::jit_trace(
            &mut ls,
            &TraceCfg {
                on: m == 2,
                hot: (2, 2),
                variant: (cfg >> 4) & 7,
                no_loop: false,
                no_fwd: false,
                tracemove: m == 3,
                no_sink: (cfg >> 4) & 7 == 3,
                seed: src.len() as u64,
            },
        );
    }
    if ls.load_chunk(src, b"=fuzz", Some(b"t")) == rn_core::status::Status::Ok {
        let f = ls.top() - 1;
        if compiled {
            // compiled code is not entered while a hook is installed: wall-clock watchdog instead
            let _ = rustynail_fuzz::run_watchdog(&mut ls, f, 0, 400);
        } else {
            let _ = run_budgeted(&mut ls, f, 0);
        }
    }
    ls.collect(false);
});
