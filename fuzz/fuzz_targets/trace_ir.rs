//! Random trace IR through the trace compiler's pure pipeline (docs/JIT_TRACE.md 9.8): the P2 generator
//! (`tr::interp::gen::random_case`, driven by a seed taken from the input) -> fold/CSE -> forwarding / peeling
//! (`opt_loop`) -> register allocation, exit snapshots, assembly -> the micro-ops run in the emulator (inline or generic
//! stubs) or natively, against the reference interpreter `tr::interp` on several random models: exit snapshot, restored
//! frame, budget word, arrays, upvalue cells. The harness is the one of `crates/rn-jit/tests/tr_asm.rs` (sliced in by
//! `fuzz/build.rs`); the byte layout is documented in `src/trace_harness_ext.rs`.
//!
//! Input: 8 bytes seed, 3 config bytes (size / loops / budget register, pass mask and backend, random allocation).
//! Shorter inputs are ignored; the generator makes every byte string a valid case. Build with `--features trace`.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    #[cfg(feature = "trace")]
    rustynail_fuzz::trace_harness::run_case(data);
    #[cfg(not(feature = "trace"))]
    {
        let _ = data;
        panic!("build with --features trace");
    }
});
