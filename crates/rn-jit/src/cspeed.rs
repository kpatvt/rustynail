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
/// The register allocator's free-register choice and eviction scan without a temporary vector per query (the pool lives on the stack),
/// and its per-snapshot set-up (unfusing the snapshot refs, the validity check, the snapshot-only marks) once per snapshot instead of once
/// per guard (the guards of a snapshot share its map, TR27).
pub const RA: u32 = 32;
/// The pipeline takes the closed recording over instead of copying it (nothing reads the recording after its compile).
pub const REC: u32 = 64;

/// Section 53: a side trace's compile computes no shape hash. The hash of a compile only identifies a re-compile of a head's root trace that
/// would repeat the generation flushed last (`cache::install` compares it for root traces; `cache::retire` keeps it for a head's own trace),
/// so a side trace's was never read: the install gets 0 ("no hash").
pub const SIDEHASH: u32 = 128;
/// Section 53: the forwarding pass of a side / stitched trace (`opt_loop::run_fwd`, which keeps the snapshots and their numbers) renames each
/// snapshot's entries into one reused buffer and prunes them in place (it built three vectors per snapshot and looked the frame links up from
/// the start of the frame list), and its clean-up keeps the snapshot tables as they are (with every snapshot kept, the rebuild was a copy).
pub const FWDSNAP: u32 = 256;
/// Section 53: allocation sinking returns before its analysis (the set of loaded slots, the candidate scan) when the trace allocates no table
/// (no `TNEW`): it found no candidate then. Most side traces allocate nothing.
pub const SINKSKIP: u32 = 512;

/// Section 76.9: the close of a recording computes the fetched count before each snapshot's step from prefix sums of the step costs
/// (`snap::fixup` summed the costs from the start once per snapshot).
pub const FIXSUM: u32 = 1024;
/// Section 76.9: the snapshot pruning of a closed recording (`snap::prune_snaps`) copies a snapshot whole when nothing in it may be dropped
/// (inlined frames, an open top, or no dead temporary at its pc) instead of testing every entry.
pub const PRUNEQ: u32 = 2048;

/// Section 80.2 (M1): the back end without per-compile copies and scans: the allocator's free register from a mask of the held registers (the
/// pool is in register order, so the first free register is the lowest bit), the assembler's stubs read the exit snapshots in place (no clone per
/// stub) and its reloads / spill stores without copying the allocation's lists, the label tables of the exit builder copied without a lookup per pc.
pub const M1: u32 = 4096;

/// Section 85.1: allocation sinking without hash sets: the analysis of a candidate marks the instructions it derives from the table in one byte
/// per instruction (it looked every operand of every later instruction up in five hash sets), and the rebuild of the snapshots derives the
/// contents of a sunk table once per prefix of its stores and its header constant once per table (it re-derived both for every snapshot).
pub const SINKD: u32 = 8192;

/// Section 85.2: the copies of the loop / forwarding pass map a constant of the source trace to the output's through a table indexed by the
/// constant ref (a hash lookup for every constant operand and snapshot entry before).
pub const FWDK: u32 = 16384;

/// All levers.
pub const ALL: u32 = MASKS
    | ENC
    | FXHASH
    | PASSES
    | RA
    | REC
    | SIDEHASH
    | FWDSNAP
    | SINKSKIP
    | FIXSUM
    | PRUNEQ
    | M1
    | SINKD
    | FWDK;
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
