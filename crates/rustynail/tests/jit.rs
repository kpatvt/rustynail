//! The optional JIT through the safe facade (`Lua::set_jit_mode`), both Lua versions: results are identical
//! across modes, the default and the sandbox preset stay interpreted, the instruction limit counts exactly,
//! and the memory limit covers the JIT's structures. Only built with the `jit` cargo feature.
#![cfg(feature = "jit")]
use rustynail::{ErrorKind, JitMode, Lua};

const PROG: &str = "
local function fib(n) if n < 2 then return n end return fib(n-1) + fib(n-2) end
local t = {}
for i = 1, 2000 do t[i] = (i * 7) % 13 end
local s = 0
for r = 1, 50 do for i = 1, #t do s = s + t[i] * r end end
local parts = {}
for i = 1, 20 do parts[#parts+1] = tostring(fib(i)) end
return s .. ':' .. table.concat(parts, ',')";

fn run(l: &mut Lua) -> String {
    l.do_string(PROG).unwrap();
    let s = l.to_string_lossy(-1).unwrap();
    l.pop(1).unwrap();
    s
}

#[test]
fn default_is_off_even_with_the_feature() {
    let mut l = Lua::new().unwrap();
    let r = run(&mut l);
    let st = l.jit_stats().expect("compiler installed");
    assert_eq!(st.compiled, 0, "{st:?}");
    assert_eq!(st.enters, 0);
    assert!(!l.set_jit_mode(JitMode::Off));
    assert!(!r.is_empty());
}

#[test]
fn sandbox_preset_does_not_enable_the_jit() {
    let mut l = Lua::builder().sandbox().build().unwrap();
    run(&mut l);
    assert_eq!(l.jit_stats().unwrap().compiled, 0);
    // an explicit request is honoured (and results do not change)
    let mut l2 = Lua::builder()
        .sandbox()
        .jit(JitMode::Always)
        .build()
        .unwrap();
    assert!(run(&mut l2).len() > 10);
    assert!(l2.jit_stats().unwrap().compiled > 0);
}

#[test]
fn results_are_identical_across_modes() {
    let mut off = Lua::builder().seed(1).build().unwrap();
    let want = run(&mut off);
    for mode in [JitMode::On, JitMode::Always] {
        let mut l = Lua::builder().seed(1).jit(mode).build().unwrap();
        assert!(l.set_jit_mode(mode), "no executable memory on this host?");
        assert_eq!(run(&mut l), want, "{mode:?}");
        let st = l.jit_stats().unwrap();
        assert!(st.compiled > 0 && st.enters > 0, "{mode:?}: {st:?}");
        // switching off stops entering compiled code
        assert!(!l.set_jit_mode(JitMode::Off));
        let before = l.jit_stats().unwrap().enters;
        assert_eq!(run(&mut l), want);
        assert_eq!(l.jit_stats().unwrap().enters, before);
    }
}

#[test]
fn errors_are_identical_across_modes() {
    let src =
        "local function f(x) return x.y.z end for i = 1, 5 do pcall(f, {y={z=1}}) end return f({})";
    let mut msgs = Vec::new();
    for mode in [JitMode::Off, JitMode::Always] {
        let mut l = Lua::builder().jit(mode).build().unwrap();
        let e = l.do_string(src).unwrap_err();
        msgs.push((e.kind(), e.to_string()));
    }
    assert_eq!(msgs[0], msgs[1]);
}

#[test]
fn instruction_limit_counts_exactly_and_disables_compiled_entry() {
    let src = "n = 0 local function bump() n = n + 1 end while true do bump() end";
    let mut seen = Vec::new();
    for mode in [JitMode::Off, JitMode::On, JitMode::Always] {
        let mut l = Lua::builder()
            .instruction_limit(300_000)
            .jit(mode)
            .build()
            .unwrap();
        let e = l.do_string(src).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::InstructionLimit, "{mode:?}");
        l.do_string("return n").unwrap();
        let n = l.to_integer(-1).unwrap();
        seen.push(n);
        let st = l.jit_stats().unwrap();
        assert_eq!(
            st.enters, 0,
            "{mode:?}: compiled code ran under a count hook"
        );
        // lifting the limit lets the JIT run again
        l.set_instruction_limit(None);
        l.do_string(
            "local s = 0 for i = 1, 300000 do s = s + i % 3 end local function g(x) return x end for i = 1, 300 do g(i) end",
        )
        .unwrap();
        if mode != JitMode::Off {
            assert!(l.jit_stats().unwrap().enters > 0, "{mode:?}");
        }
    }
    assert!(seen[0] > 10_000, "{seen:?}");
    assert!(seen.iter().all(|&n| n == seen[0]), "{seen:?}");
}

#[test]
fn memory_limit_covers_jit_structures_and_degrades_gracefully() {
    let prog = "local t = {}
        for i = 1, 150 do t[i] = load('local a = ... return function(x) return x * a + 1 end')(i) end
        local s = 0
        for r = 1, 5 do for i = 1, #t do s = s + t[i](r) end end
        return s";
    let mut want = None;
    let mut completed = 0;
    for head in [0usize, 1_000, 10_000, 40_000, 120_000, 400_000, 8_000_000] {
        let mut l = Lua::builder().jit(JitMode::Always).build().unwrap();
        l.do_string("collectgarbage()").unwrap();
        let base = l.memory_used();
        l.set_memory_limit(Some(base + head));
        match l.do_string(prog) {
            Ok(_) => {
                let v = l.to_integer(-1).unwrap();
                assert_eq!(*want.get_or_insert(v), v, "head {head}");
                completed += 1;
            }
            Err(e) => assert!(
                matches!(e.kind(), ErrorKind::Memory | ErrorKind::Runtime),
                "head {head}: {e}"
            ),
        }
        // the state is still usable afterwards
        l.set_memory_limit(None);
        l.do_string("return 1 + 1").unwrap();
    }
    assert!(completed >= 2, "{completed}");
}

// ---- F9: traces (docs/JIT_TRACE.md D3/TR31) --------------------------------------------------------------------

#[test]
fn builder_jit_trace_switch() {
    let mut off = Lua::builder().seed(1).build().unwrap();
    let want = run(&mut off);
    let process_on = std::env::var("RUSTYNAIL_JIT_TRACE")
        .map_or(true, |v| !matches!(v.trim(), "off" | "0" | "false"))
        && std::env::var("RUSTYNAIL_JIT").map_or(true, |v| v.trim() != "off");
    for (flag, mode) in [
        (None, JitMode::Always),
        (Some(true), JitMode::On),
        (Some(false), JitMode::Always),
    ] {
        let mut b = Lua::builder().seed(1).jit(mode);
        if let Some(f) = flag {
            b = b.jit_trace(f);
        }
        let mut l = b.build().unwrap();
        assert_eq!(run(&mut l), want, "{flag:?} {mode:?}");
        // the per-state switch reports whether traces can run (default on unless the process turned them off)
        assert_eq!(l.set_jit_trace(true), process_on);
        assert!(!l.set_jit_trace(false));
        assert_eq!(run(&mut l), want, "after switching traces off");
    }
}
