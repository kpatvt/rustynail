//! Template oracle (throwaway prototype, docs/JIT_ORACLE.md): how many cycles does a plain template JIT
//! save versus the interpreter on numeric loops?
//!
//! usage: `oracle <mode> <file.lua> [N]`
//!   mode: `dump` (print decoded bytecode), `v1` (plain templates), `v1o` (v1 + back-edge poll + block
//!         counter), `v2` (whole-function register allocation; types from static inference), `compile`
//!         variants `v1c`/`v1oc`/`v2c` only compile (measures the fixed process cost).
//!
//! Files that mention `ORACLE` follow the State-based protocol of `staterun.rs` (tables and calls: the file
//! runs in a real `rn-core` State, hot functions are compiled from its protos; modes `v1`, `v1o`, `v2`, `dump`).
//!
//! The function compiled is the first nested proto of the file (`local function f(N) ... end`); it is run
//! once with `N` in register 0, on a hand-made frame in the interpreter's Value layout, no State/heap/GC.
#![allow(dead_code)]

mod decode;
mod staterun;
mod v1;
mod v2;

use rn_jit::mem::{probe_exec, CodeArena};
use std::time::Instant;
use v1::{St, FLT, INT};

type Entry = extern "C" fn(*mut u8, *const u8, *mut St) -> u64;

fn fmt_lua_float(x: f64) -> String {
    // lobject.c tostringbuff (5.5): "%.14g"; if that does not read back as `x`, "%.17g"; then ".0" is
    // added if the result looks like an int
    let fmt = |f: &std::ffi::CStr| {
        let mut buf = [0u8; 64];
        // SAFETY: `f` is a NUL-terminated format taking one double, `buf` is large enough for "%.17g" of
        // any double, and snprintf writes at most `buf.len()` bytes including the NUL.
        let n = unsafe { libc::snprintf(buf.as_mut_ptr().cast(), buf.len(), f.as_ptr(), x) };
        String::from_utf8_lossy(&buf[..n as usize]).into_owned()
    };
    let mut s = fmt(c"%.14g");
    if s.parse::<f64>().ok() != Some(x) {
        s = fmt(c"%.17g");
    }
    if !s.contains(['.', 'e', 'n', 'i']) {
        s.push_str(".0");
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: oracle <dump|v1|v1o|v2|v1c|v1oc|v2c> file.lua [N]");
        std::process::exit(2);
    }
    let mode = args[1].as_str();
    let src = std::fs::read(&args[2]).expect("read source");
    let n: i64 = args.get(3).map_or(0, |s| s.parse().expect("N"));
    if src.windows(6).any(|w| w == b"ORACLE") {
        // State-based programs (tables and calls)
        staterun::run(mode, &src, n, mode == "dump");
        return;
    }
    let func = decode::compile_source(&src).expect("compile");
    if mode == "dump" {
        for (pc, op) in func.ops.iter().enumerate() {
            println!("{pc:3} {op:?}");
        }
        println!("k = {:?}, max_stack = {}", func.k, func.max_stack);
        return;
    }

    let t0 = Instant::now();
    let (only_compile, mode) = match mode.strip_suffix('c') {
        Some(m) if m.starts_with('v') => (true, m),
        _ => (false, mode),
    };
    let compiled = match mode {
        "v1" => v1::compile(&func, v1::Opts::default()),
        "v1o" => v1::compile(
            &func,
            v1::Opts {
                poll: true,
                count: true,
            },
        ),
        "v2" => v2::compile(&func),
        _ => panic!("unknown mode {mode}"),
    };
    probe_exec().expect("executable memory");
    let mut arena = CodeArena::new();
    let mut block = arena.alloc(compiled.code.len(), 64).expect("alloc");
    arena
        .writer(&mut block)
        .expect("writer")
        .write_at(0, &compiled.code);
    arena.commit().expect("commit");
    // SAFETY: the block holds a function with the C ABI signature `Entry` produced by the oracle compiler
    // (System V: rdi = base, rsi = kcache, rdx = state); it stays alive until the end of `main`.
    let entry: Entry = unsafe { arena.entry(&block) }.expect("entry");
    let compile_time = t0.elapsed();
    eprintln!(
        "[oracle] {mode}: {} bytes of code, compile+map {:?}",
        compiled.code.len(),
        compile_time
    );
    if only_compile {
        return;
    }

    // hand-made frame: 16 bytes per slot, tag word then payload
    let mut frame: Vec<[u64; 2]> = vec![[0, 0]; func.max_stack + 8];
    frame[0] = [INT as u64, n as u64];
    let kv: Vec<[u64; 2]> = func
        .k
        .iter()
        .map(|k| match *k {
            decode::Kv::Int(i) => [INT as u64, i as u64],
            decode::Kv::Flt(f) => [FLT as u64, f.to_bits()],
            _ => [0, 0],
        })
        .collect();
    let mut st = St {
        trap: 0,
        _pad: 0,
        count: i64::MAX,
        abort_pc: 0,
        abort_fn: 0,
        ci: std::ptr::null_mut(),
        top: 0,
        stack_size: 0,
        stack_base: std::ptr::null_mut(),
    };
    let t1 = Instant::now();
    let bits = entry(
        frame.as_mut_ptr().cast(),
        kv.as_ptr().cast(),
        &mut st as *mut St,
    );
    let run = t1.elapsed();
    if st.abort_pc != 0 {
        let pc = st.abort_pc as usize - 1;
        eprintln!(
            "[oracle] ABORT at pc {pc}: {:?} (type mismatch or unsupported case)",
            func.ops.get(pc)
        );
        std::process::exit(3);
    }
    let tag = match compiled.ret_ty {
        Some(t) => t as u64,
        None => frame[compiled.ret_reg as usize][0] & 0xFF,
    };
    eprintln!("[oracle] run {:?}", run);
    if tag == INT as u64 {
        println!("{}", bits as i64);
    } else if tag == FLT as u64 {
        println!("{}", fmt_lua_float(f64::from_bits(bits)));
    } else {
        println!("unexpected result tag {tag}");
        std::process::exit(4);
    }
}
