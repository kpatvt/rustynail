//! Section 49 of docs/JIT_TRACE.md (C1): the compile-speed levers of the trace back end and of the shared x86 encoder.
//!
//! Every lever produces the same machine code as the path it replaces (the knob exists so that a lever can be switched off and so that
//! `ccheck` can compare the two paths in one process). The bits come from the `cspeed` knob of `RUSTYNAIL_JIT_TRACE_LIMITS`
//! (`TrLimits::cspeed`); the trace pipeline and the tier-1 compile store them in a thread-local cell before they compile, the code that
//! has a lever reads the cell. Code compiled outside both (unit tests of the encoder, the thunk) sees [`DEFAULT`].

use std::cell::Cell;

/// `Allocation::uses_gpr` / `uses_xmm` from register masks built by one scan (instead of one scan of the allocation per query).
pub const MASKS: u32 = 1;
/// The x86 encoder (traces and tier 1): its buffers presized from the program, the `[REX] opcode ModRM [SIB] [disp]` bytes of an
/// instruction written in place (the encoding routine inlined into every emitter, so the operand stays in registers) instead of one
/// checked push per byte.
pub const ENC: u32 = 2;
/// The shape hash of a compile (`cache::shape_hash_ir`) with the FxHash word mix instead of the FNV variant that mixed every byte of a
/// slice separately (the value is only compared with other values of the same process).
pub const FXHASH: u32 = 8;
/// The passes and the assembler without their quadratic scans and regrowth: sinking's "store between two guards of one snapshot" check
/// from an index of the snapshots that name each candidate (it scanned every snapshot map once per store of every candidate), its candidate
/// lookup per instruction from the candidate map; the loop pass's output and the assembler's micro-op program presized from the input.
pub const PASSES: u32 = 16;
/// The register allocator's free-register choice and eviction scan without a temporary vector per query (the pool lives on the stack).
pub const RA: u32 = 32;
/// The pipeline takes the closed recording over instead of copying it (nothing reads the recording after its compile).
pub const REC: u32 = 64;

/// All levers.
pub const ALL: u32 = MASKS | ENC | FXHASH | PASSES | RA | REC;
/// The process default of `TrLimits::cspeed`.
pub const DEFAULT: u32 = ALL;

thread_local! {
    static BITS: Cell<u32> = const { Cell::new(DEFAULT) };
}

/// Is lever `bit` on for the compile running on this thread?
#[inline(always)]
pub fn on(bit: u32) -> bool {
    BITS.with(|b| b.get() & bit != 0)
}

/// The levers of the compile about to run on this thread.
#[inline]
pub fn set(bits: u32) {
    BITS.with(|b| b.set(bits));
}

/// The levers in effect.
#[inline]
pub fn get() -> u32 {
    BITS.with(Cell::get)
}

/// Runs `f` with the levers `bits` and restores the previous ones (the `ccheck` comparison runs the back end a second time with 0).
pub fn with<T>(bits: u32, f: impl FnOnce() -> T) -> T {
    let old = get();
    set(bits);
    let r = f();
    set(old);
    r
}
