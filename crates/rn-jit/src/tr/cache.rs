//! J5 trace runtime (docs/JIT_TRACE.md sections 3.1-3.6, task P6): configuration, the per-prototype [`TrProto`]
//! (dispatch cells, counters, penalty state), the per-state trace cache with generation-checked [`TrId`]s, dependency
//! edges (TR32), flush (TR18), charging (3.5) and the tier-1 lowering hook that emits the dispatch jumps and counting
//! stubs (TR20).
//!
//! Nothing here records, optimises or assembles a trace (P3a, P5b, PI do): the compile pipeline reaches this module
//! through [`begin_recording`] / [`abort_recording`] / [`install`] and the recorder through [`StartFn`].
//!
//! # Machine-code contract (docs/UNSAFE.md paragraph in the P6 report)
//!
//! * `cell[H]` is a `usize` in a boxed, never reallocated [`HeadData`]; tier-1 code embeds its address. Its target is
//!   a function of the head state ([`HeadData::target`]): the counting stub `C_H` (Warm, Recording), the trace entry
//!   or the `+tracemove` stub (Traced), `pcl[H]` (Black). Only Rust writes it, always to an address inside a live
//!   code block of the *current* tier-1 version or inside a live trace block.
//! * Dispatch `D_H` is exactly `Mov V7, &cell; Ld V7, [V7]; JmpR V7`; `C_H` falls into `pcl[H]` (downstream of `D_H`)
//!   or calls [`jit_tr_hot`].

use super::exit::ExitMeta;
use super::print;
use super::{mutants, TraceOpts};
use crate::ir::{Base, Cc, Op, Program, Src, M, W};
use crate::lower::{Cx, Kind, Lowered, RegionEnv, RegionOut};
use crate::mem::CodeBlock;
use crate::rt::{self, Fail, JitData, JitProto};
use crate::x86::{self, Emitted};
use rn_core::jit_hooks;
use rn_core::state::{CallInfo, Ci, State, Thread};
use rn_core::ver::active::OpCode;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::OnceLock;

// ---- configuration ----------------------------------------------------------------------------------------

/// `RUSTYNAIL_JIT_TRACE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrMode {
    /// No dispatch cells, no counters: tier-1 code is byte-identical to the build without traces.
    Off,
    /// Record hot loops (thresholds [`TrConfig::hot_loop`], [`TrConfig::hot_exit`]).
    On,
    /// Record at the first back-edge, side traces at the second exit, deterministic penalties.
    Always,
}

/// `RUSTYNAIL_JIT_TRACE_PREF` (TR19b, experiment X5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrPref {
    /// Trace first, J4 region after blacklisting (the shipped default, D5).
    Trace,
    /// Region first (every head of a J4 nest starts `Black`), the tracer after J4 gave the nest up.
    Region,
}

/// P-F: the process default of [`TrLimits::sidemin`].
pub const SIDEMIN_PROCESS: u32 = 4;
/// BUNDLE-L A: the process default of [`TrLimits::aidx`] (also the struct default).
pub const AIDX_PROCESS: u32 = 0;
/// BUNDLE-L B: the process default of [`TrLimits::bhoist`] (also the struct default).
pub const BHOIST_PROCESS: u32 = 1;
/// BUNDLE-K L1: the process default of [`TrLimits::lazyci`] (also the struct default).
pub const LAZYCI_PROCESS: u32 = 0;
/// P-M4: the process default of [`TrLimits::recnat`] (native self-recursion on; K3a: also the struct default).
pub const RECNAT_PROCESS: u32 = 1;
/// R2: the process default of [`TrLimits::recany`] (K3a: also the struct default).
pub const RECANY_PROCESS: u32 = 1;
/// R2: the process default of [`TrLimits::siderooms`].
pub const SIDEROOMS_PROCESS: u32 = 1;
/// R2: the process default of [`TrLimits::nilstore`].
pub const NILSTORE_PROCESS: u32 = 1;
/// T3: process default of [`TrLimits::t3idx`] (two inline `__index` levels in the tier-1 table templates; K3a: also the struct default).
pub const T3IDX_PROCESS: u32 = 2;
/// BUNDLE-P (`laprune`): process value of [`TrLimits::laprune`] (K3a: also the struct default): when the `loopany` guard fires for a family whose root
/// trace unrolled no callee loop itself, only the side traces that did are flushed (the root and the other side traces stay) instead of the whole family.
pub const LAPRUNE_PROCESS: u32 = 1;
/// C36 (`cispare`): process value of [`TrLimits::cispare`] (K3a: also the struct default): a persistent entry failure whose cause is missing spare `CallInfo` nodes
/// behind the frame (`TR6b`) is repaired by extending the chain instead of flushing the root.
pub const CISPARE_PROCESS: u32 = 1;
/// C36 (`domtop`): process value of [`TrLimits::domtop`] (K3a: also the struct default): the percent of a root's entries through ONE pre-roll site into a side
/// trace (at least [`TrLimits::domn`] entries) above which the root, recorded on a rare path, is re-recorded once (no penalty).
pub const DOMTOP_PROCESS: u32 = 82;
/// C36 (`domn`): process value of [`TrLimits::domn`]: entries a root needs before the `domtop` rule may fire.
pub const DOMN_PROCESS: u32 = 65536;
/// P2 fix (`pollr15`): process value of [`TrLimits::pollr15`]. **2 = on except in traces of native self-recursion** (regression fix of 2026-11-12: the P2 fix had made it 0, which cost 1 instruction per loop iteration of every other trace: lib/m_*, int_sum +5..13 %). 0 = off, 1 = on everywhere. The r15-addressed poll makes `programs/fib` 7-9 % slower in cycles than the
/// `movabs` form at every loop-head offset (docs/PERF.md "P2 loop-top placement fix"); `RUSTYNAIL_JIT_TRACE_LIMITS=pollr15=1` turns it on (all 223 programs 0.985 / 0.971 on, 0.989 / 0.983 off).
pub const POLLR15_PROCESS: u32 = 2;
/// P2 bundle 1 (`xregs`): process value of [`TrLimits::xregs`] (K3a: the process value is also the struct default): bit 0 = `rbp` (`V9`), bit 1 = `r14` (`V10`) are allocatable in traces. **0 = off:**
/// the A/B gate of the bundle is not clean (docs/PERF.md "P2 bundle 1"); `RUSTYNAIL_JIT_TRACE_LIMITS=xregs=3` turns both on (geomean over the 223 programs: instructions
/// 0.985, cycles 0.982; `jit-trace/field_method` 0.69 / 0.76), and this constant is the one-line switch.
pub const XREGS_PROCESS: u32 = 0;
/// R1 (`rearm`): process value of [`TrLimits::rearm`] (K3a: also the struct default). **4:** the arming lasts four back-edges of the armed prototype's own loops
/// (all 223 programs, cycles 0.996, `corpus/class_middleclass` 0.75, `corpus/classic_vector` 0.89; 16 and 64 are within 0.2 %; docs/PERF.md "R1 rearm"); `RUSTYNAIL_JIT_TRACE_LIMITS=rearm=0` turns it off.
pub const REARM_PROCESS: u32 = 4;
/// R1: consecutive short rearm stays of one head before rearm entries into it are refused ([`HeadData::rearm_budget`]); one more try after [`REARM_WAIT`] refused attempts.
pub const REARM_BUDGET: u8 = 8;
/// R1: a head is only entered by a rearmed tick once its root trace has been entered this often (`HeadData::entry_ok`, since the install): a young trace is
/// still being shaped by its exits (side traces, re-records, the ineffective rule) and the rearmed entries, which come far more often than ticks, would change when
/// those decisions fall (`programs/heapsort`, `corpus2/dev/iter_pipeline` lost their loop traces that way).
pub const REARM_MIN_ENTRIES: u32 = 1024;
/// BUNDLE-P P1 / C45 (`rearmside`): process value of [`TrLimits::rearmside`] (K3a: also the struct default). A head whose root trace has fewer than [`REARM_MIN_ENTRIES`]
/// entries is nevertheless entered by a rearmed tick when its family already has this many installed side traces and (with [`REARMCLEAN_PROCESS`]) is clean (0 = never: the R1
/// rule). See [`rearm_young_ok`].
/// **2 (C45, with `rearmclean=1`):** `corpus/classic_vector` 0.86 cycles; without the clean test (`rearmclean=0`, the plain BUNDLE-P rule) `rearmside=3` cost
/// `corpus/lua_lexer` +4.8 % instructions, `awfy/json` +1.3 %, `awfy/queens` +3.2 % cycles (formation shifts; docs/PERF.md "BUNDLE-P", "BUNDLE-C45").
pub const REARMSIDE_PROCESS: u32 = 2;
/// C45 (`rearmclean`): process value of [`TrLimits::rearmclean`] (also the struct default): 1 = a young head is entered by a rearmed tick only when its family is *clean*: no
/// failed side recording so far (`Trace::side_faults`) and no member trace that unrolled a callee loop (`Trace::la_any`). Families that already had a failed side recording or
/// unrolled a callee loop (`heapsort`, `iter_pipeline`, `awfy/queens`, `corpus/lua_lexer`, `awfy/json`) are shaped by their exits for longer and the rearmed entries
/// change when those decisions fall; 0 = the plain `rearmside` rule.
pub const REARMCLEAN_PROCESS: u32 = 1;
/// BUNDLE-P P1 / C45: may a rearmed tick enter a head whose root trace has been entered `entry_ok` times and whose family holds `family_sides` installed side traces
/// (`clean`: see [`REARMCLEAN_PROCESS`]; ignored when `rearmclean` is 0)?
/// R1: after [`REARM_MIN_ENTRIES`] entries. P1: or, with `rearmside` = `k` > 0, as soon as `k` side traces are installed (the trace is being shaped productively;
/// the entries of the rearmed ticks are what makes its exits hot, and waiting for 1024 entries from the 1000-edge formation tick starves the cascade of side traces:
/// `corpus/classic_vector` needed 255 of its 1000 steps to get there). C45: and the family is clean.
pub fn rearm_young_ok(
    entry_ok: u32,
    family_sides: u32,
    clean: bool,
    rearmside: u32,
    rearmclean: u32,
) -> bool {
    if entry_ok >= REARM_MIN_ENTRIES {
        return true;
    }
    if rearmside == 0 || mutants::active("rearmside_never") {
        return false;
    }
    if family_sides < rearmside && !mutants::active("rearmside_any") {
        return false;
    }
    clean || rearmclean == 0
}
/// R1: refused rearm attempts after which a head whose budget ran out gets one more try.
pub const REARM_WAIT: u8 = 64;
/// R1: compiled bytecodes executed between two rearm entries into a head below which the stay in between counts as short.
pub const REARM_SHORT: u64 = 64;
/// P3 (`ncallee`): process value of [`TrLimits::ncallee`] (K3a: also the struct default): after an entry-failure flush of a root whose entry segment holds a callee
/// identity guard, the root is recorded again with its callee guards kept out of the entry segment (a callee mismatch becomes an ordinary exit with a side trace).
pub const NCALLEE_PROCESS: u32 = 1;
/// P3 (`chainre`): process value of [`TrLimits::chainre`] (K3a: the process value is also the struct default): the percent of a side trace's entries that must leave through its callee-guard exit (pass-through)
/// before the child subtree of that exit may be re-ordered.
pub const CHAINRE_PROCESS: u32 = 95;
/// P3 (`chainn`): process value of [`TrLimits::chainn`]: entries a side trace needs before the `chainre` rule may fire.
pub const CHAINN_PROCESS: u32 = 65536;
/// BUNDLE-C (`sbump`): process value of [`TrLimits::sbump`] (K3a: also the struct default): extra recursion unroll granted to the retry of a side attempt
/// whose previous attempt on the same exit ended `SideShort` inside inlined frames.
pub const SBUMP_PROCESS: u32 = 1;
/// BUNDLE-D (`ccalls`): process value of [`TrLimits::ccalls`] (K3a: also the struct default). **1:** all 223 programs cycles 1.000 / instructions 0.999,
/// `corpus/class_middleclass` 0.890, `programs/metamethods` 0.933, `corpus/astar_jumper` 0.975, nothing above noise (docs/PERF.md "BUNDLE-D"); `ccalls=0` turns it off.
pub const CCALLS_PROCESS: u32 = 1;
/// BUNDLE-P P2 (`f2ifl`): process value of [`TrLimits::f2ifl`] (K3a: also the struct default). **1:** the float-to-integer guard behind `FFLOOR` is `cvttsd2si; cmp r, 1; jo` (docs/PERF.md "BUNDLE-P").
pub const F2IFL_PROCESS: u32 = 1;
/// BUNDLE-Q (`t1q`): process value of [`TrLimits::t1q`] (K3a: also the struct default), a bit mask: **1** tier-1 `EQ` of two tables / userdata / functions answers raw-equal
/// and no-`__eq` pairs itself (inline: same payload, or both metatables absent / flagged without `__eq`; `jit_cmp` for the rest), **2** tier-1 `MOD` / `MODK` with a float
/// operand calls `jit_fmod` instead of leaving (docs/PERF.md "BUNDLE-Q"), **4** (BUNDLE-S) a tier-1 table read whose `__index` is a Lua function runs the handler as an in-place
/// frame of compiled code, **8** the same for `__newindex` stores (docs/PERF.md "BUNDLE-S"), **16** (BUNDLE-V) arithmetic / bitwise / unary metamethod frames (`Cold::ArithMm`, private
/// continuation entries), **32** `TAILCALL` inside a metamethod frame stays in compiled code, **64** the general `RETURN` (several / open results, vararg frames) of a metamethod frame
/// does (docs/PERF.md "BUNDLE-V"). Process value 125 = 1 + 4 + 8 + 16 + 32 + 64; a named value is clamped to 127.
pub const T1Q_PROCESS: u32 = 125;
/// BUNDLE-S S2 (`amt`): process value of [`TrLimits::amt`] (K3a: also the struct default). **1:** array reads / stores of tables with a metatable are recorded (docs/PERF.md "BUNDLE-S").
pub const AMT_PROCESS: u32 = 1;
/// BUNDLE-D (`armm`): process value of [`TrLimits::armm`] (K3a: the process value is also the struct default). **0:** `programs/metamethods` 2.24x cycles (the tier-1 continuation keeps the loop out of the trace), nothing gains (docs/PERF.md "BUNDLE-D").
/// Superseded by `t1q` bit 4 (BUNDLE-V: the same machinery with private continuation entries, no formation effect, `classic_vector` 0.67); the limit itself stays 0.
pub const ARMM_PROCESS: u32 = 0;
/// BUNDLE-D (`sbump2`): process value of [`TrLimits::sbump2`] (K3a: the process value is also the struct default). **0:** `corpus/class_middleclass` 1.008, `awfy/list` +22 %, `awfy/towers` +22 % (docs/PERF.md "BUNDLE-D").
pub const SBUMP2_PROCESS: u32 = 0;
/// BUNDLE-F (`stitch0`): process value of [`TrLimits::stitch0`] (K3a: the process value is also the struct default): a side recording that would stitch after 0 bytecodes is refused.
pub const STITCH0_PROCESS: u32 = 1;
/// BUNDLE-F (`fibdiet`): process value of [`TrLimits::fibdiet`] (K3a: the process value is also the struct default), a bit mask of the pieces of the slimmer native-recursion call protocol.
pub const FIBDIET_PROCESS: u32 = 0;
/// BUNDLE-G (`chaindepth`): process value of [`TrLimits::chaindepth`] (0 = unlimited chains). **4:** no program of the 223 changes by a single instruction, fdisp / mdisp K = 16..32 -5 .. -40 % cycles (docs/PERF.md "BUNDLE-G").
pub const CHAINDEPTH_PROCESS: u32 = 4;
/// BUNDLE-C (`fnpromo`): process value of [`TrLimits::fnpromo`] (K3a: also the struct default).
pub const FNPROMO_PROCESS: u32 = 6;
/// BUNDLE-C (`fnpromow`): process value of [`TrLimits::fnpromow`].
pub const FNPROMOW_PROCESS: u32 = 3;
/// C36 (`second`): process value of [`TrLimits::second`] (K3a: also the struct default): the second chances of a head lost to a transient (bit 0 abort-count retry after `LeaveFrame`, bit 1 first identical re-record kept).
pub const SECOND_PROCESS: u32 = 3;
/// C39 (`innerretry`): process value of [`TrLimits::innerretry`] (K3a: also the struct default): retry rounds of a head blacklisted by `InnerLoop` aborts.
pub const INNERRETRY_PROCESS: u32 = 6;
/// C39 follow-up: extra doublings of the wait of every `innerretry` round (x4): a round costs one aborted recording and, when it succeeds,
/// a compile of a loop the first rounds could not record; the wait amortises that over the interpreted back-edges (apps/expr_lang: the 2nd
/// round, after 8k back-edges, installed an ineffective 84 KB root: +30 % instructions in a 100 ms program; lua_lexer runs 10^6 back-edges).
pub const INNERRETRY_WAIT_SHIFT: u8 = 2;
/// C38 (`sidenest`): process value of [`TrLimits::sidenest`] (K3a: also the struct default): a side trace that reaches a callee loop head with a root trace ends with a linking exit.
pub const SIDENEST_PROCESS: u32 = 1;
/// DYNMASK: process default of [`TrLimits::dynmask`] (a dynamic short-string key is looked up with the hash mask and node vector read from the table at run time: no `lsizenode` guard, a load that misses reads a static nil node instead of exiting; K3a: also the struct default).
pub const DYNMASK_PROCESS: u32 = 1;
/// BUNDLE-M item C1: process default of [`TrLimits::lockstep`] (0 = the recorder's lockstep cross-check (TR17) is off in a process; 1 = every recording; K >= 2 = one recording in K; the struct default, used by the tests, is 1; `+verify` and debug builds always run it).
pub const LOCKSTEP_PROCESS: u32 = 0;
/// BUNDLE-T1I: process default of [`TrLimits::t1inline`] (bit mask of the built-ins the tier-1 `CALL` template runs inline, see `lower::t1inline::B_*`; 1023 = all rows, the default since C36: the `math.floor` bit used to cost `heldout/bigint_rsa` 1.3x through spare-`CallInfo` entry failures (cured by `cispare`, docs/JIT_PARKED.md C36); `t1inline=1021` leaves floor out, `t1inline=0` turns the feature off. The struct default, used by the tests, is 0.
pub const T1INLINE_PROCESS: u32 = 1023;
/// P2: process defaults of [`TrLimits::ineffcover`] and [`TrLimits::typesides`] (K3a: also the struct default).
pub const INEFFCOVER_PROCESS: u32 = 1;
pub const TYPESIDES_PROCESS: u32 = 1;
/// P-G: `TrLimits::warm` of the process configuration.
pub const DEFAULT_WARM: u32 = 1_200_000;
/// BUNDLE-1: process default of [`TrLimits::wgate`] (back-edges).
pub const WGATE_PROCESS: u32 = 30_000;
/// BUNDLE-1: process default of [`TrLimits::tailhook`].
pub const TAILHOOK_PROCESS: u32 = 1;
/// BUNDLE-C (`t1ccall`): process default of [`TrLimits::t1ccall`] (lean tier-1 call of light C functions).
pub const T1CCALL_PROCESS: u32 = 0;
/// BUNDLE-C (`tfmt`): process default of [`TrLimits::tfmt`] (`string.format` of simple constant formats recorded).
pub const TFMT_PROCESS: u32 = 1;
/// BUNDLE-N: process default of [`TrLimits::retf`] = 3: bit 0 (1) = `RETF` in side recordings (the root frame is popped and the recording goes on in the caller), bit 1 (2) = the
/// frame-linking exit of a side trace is an inline stub (the "re-push"); bit 2 (4) = function-entry families stays OFF (regresses native-recursion families, docs/JIT_PARKED.md C38).
/// Flipped from 2 to 3 on 2026-11-10 with `retfmax` 1 (docs/PERF.md "RETF flip"); the lua_lexer gate of the N3 plan was waived.
pub const RETF_PROCESS: u32 = 3;
/// BUNDLE-N: process default of [`TrLimits::retfmax`].
pub const RETFMAX_PROCESS: u32 = 1;
/// BUNDLE-V V2 (`upbar`): process value of [`TrLimits::upbar`] (K3a: also the struct default). **1:** `SETUPVAL` of a collectable value into an upvalue cell is recorded
/// (`USTORE` + `TBAR` with the upvalue write barrier); 0 = the recording aborts `Nyi(Setupval)` as before.
pub const UPBAR_PROCESS: u32 = 1;
/// BUNDLE-H / BUNDLE-R (`libcall`): process default of [`TrLimits::libcall`] (**1023** after BUNDLE-R; 7 in BUNDLE-H). Bit 0 = the missing library calls recorded: n-ary `string.char`, `math.random()`,
/// `string.lower` / `upper` / `reverse`; bit 1 = lean direct entries of the `LIB` helpers; bit 2 = inline `type`, string compares and `math.random`; bit 3 = `string.rep` recorded; bit 4 = its result length
/// class is a guard on the computed length (side traces); bit 5 = `table.unpack` of a small table inline; bit 6 = `table.move` of a small range inline; bit 7 = `tonumber` of a non-numeral string;
/// bit 8 = concatenated string keys through `HREFV`; bit 9 = lean entry of `LIB find`.
pub const LIBCALL_PROCESS: u32 = 1023;
/// BUNDLE-J (`centry`): a C-entered function root whose trace has fewer IR instructions than this does not repay the entry from C (the interpreter runs the
/// calls; measured: entering compiled code from C costs about 280 instructions per call more than the interpreter's own entry, so the light roots of `a > b` (6 IR), a `pcall`'d
/// function (5-23 IR), a coroutine body (19), a gsub callback (30) and the 46 IR roots of `serpent_ser` lose or gain nothing; the object comparators of `oop_mixins` (70, 76 IR) win
/// 0.7 x cycles; 60 is the break-even of the entry cost (about 20 bytecodes of the interpreter) against the bytecodes a root of that size covers).
pub const CENTRY_MIN_IR: u32 = 60;
/// BUNDLE-J (`centrywait`): a C-entered function is compiled with its head at its first hot event from C but entered only from its `CENTRY_WAIT`-th (a function that enters at once
/// shifts the recording order of the other roots: `corpus/msgpack_pure` +4 % instructions).
pub const CENTRY_WAIT: u32 = 8;
/// BUNDLE-J (`centry`): process default of [`TrLimits::centry`] (0 = off, 1 = on, 2 = also without the entry back-off).
pub const CENTRY_PROCESS: u32 = 2;
/// BUNDLE-H (`tnextinl`): process default of [`TrLimits::tnextinl`] (bit 0 = `TNEXT` is header-safe: the loops of `pairs` / `next` peel).
pub const TNEXTINL_PROCESS: u32 = 1;
/// BUNDLE-1: process default of [`TrLimits::hdrlib`].
pub const HDRLIB_PROCESS: u32 = 2;

/// J2-2: process default of [`TrLimits::deep`] (0 = off).
pub const DEEP_PROCESS: u32 = 8;

/// BUNDLE-2: process default of [`TrLimits::sinkmt`]. SINK-F: 1 (an exit inside inlined frames that names sunk tables gets a side trace now).
pub const SINKMT_PROCESS: u32 = 1;
/// BUNDLE-5: process default of [`TrLimits::sinkside`]. SINK-F: still 0 (sinking in side traces costs `classic_vector` 1.05 x against 0.89 x without, docs/JIT_PARKED.md A8).
pub const SINKSIDE_PROCESS: u32 = 0;
/// J1-2: process default of [`TrLimits::sinkstub`]. SINK-F: 1.
pub const SINKSTUB_PROCESS: u32 = 1;
/// SINK-R: process default of [`TrLimits::sinkroot`] (2: the shape is demoted for the root head only).
pub const SINKROOT_PROCESS: u32 = 2;
/// BUNDLE-P: process value of [`TrLimits::hoptop`]: a root through which 98 % of the entries leave by one pre-roll site into a side trace (4 checks in a row) is a
/// trampoline and is flushed. Counters only. Measured top-site shares at the checks of the 223 benchmark programs: `oop_methods` 99 % (the one program that needs the
/// rule: 3.8x slower without it), `ecs_world` 99 % (fires, neutral), `scheme_interp` 96 %, `deltablue` 95 % (flushing those two moves them by 0.88x / 1.43x in
/// instructions, so they must stay below the limit); every other program is below 90 %.
pub const HOPTOP_PROCESS: u32 = 98;
/// BUNDLE-B item 3: process value of [`TrLimits::maxside`] (K3a: also the struct default; it was 64): live side traces per family. Past the 16th, a side trace
/// costs its recording and compile (about 150k cycles each) and hops through exits that tier 1 serves as cheaply (json: 202 side traces, 185 -> 165 Mcyc with
/// 16 and 169 in tier 1 alone; queens 163 -> 142, deltablue 120 -> 115; richards, cd, bounce, havlak unchanged within noise). 8 loses on bounce and richards.
pub const MAXSIDE_PROCESS: u32 = 16;
/// BUNDLE-O: process values of [`TrLimits::polyside`] / [`TrLimits::polywait`] / `polyframes` (K3a: also the struct default).
/// **`POLYSIDE_PROCESS` is 48 (on, user decision): the measured configuration `polyside=48` (with `polywait` 2, `polyframes` 2) took `class_middleclass` to
/// 0.40 x (corpus 0.955, the 173 existing programs 1.002) at the price of `awfy/queens` 1.09 x and `awfy/cd` 1.04 x (docs/JIT_TRACE_REF.md, BUNDLE-O).
/// Accepted because OO-style code is the realistic case. One constant (0) turns it off.**
pub const POLYSIDE_PROCESS: u32 = 48;
/// See [`POLYSIDE_PROCESS`].
pub const POLYWAIT_PROCESS: u32 = 2;
/// See [`POLYSIDE_PROCESS`].
pub const POLYFRAMES_PROCESS: u32 = 2;
/// BUNDLE-S: process value of [`TrLimits::loopany`] (K3a: also the struct default): `while` / `repeat` loops of inlined callees are unrolled, guarded by
/// [`TrLimits::loopanymax`] (a family whose exits inside callee frames keep needing side traces loses its unrolled loops). `iter_pipeline` 557 -> 70 ms; the
/// unguarded `loopany=1` cost `heapsort` 1.48 x and `awfy/cd` 1.50 x (docs/JIT_PARKED.md A25). One constant (0) turns it off.
pub const LOOPANY_PROCESS: u32 = 1;
/// BUNDLE-B item 2: process value of [`TrLimits::tset`] (the three inline store paths and, BUNDLE-G1, bit 3 = the inline integer-key hash probe of `GETTABLE`; K3a: also the struct default).
pub const TSET_PROCESS: u32 = 15;

/// Is the limit `key` named in the `RUSTYNAIL_JIT_TRACE_LIMITS` string `s` (exact key match of an item `key=value`; the items are separated
/// like in [`TrLimits::parse`])? A process default is skipped only for a knob that is named. (Before K1 this was a substring test:
/// `deepins=` switched the `deep` default off, `monwarm=` counted as `warm`.)
pub fn names_limit(s: Option<&str>, key: &str) -> bool {
    s.is_some_and(|s| {
        s.split([' ', ','])
            .any(|w| w.split_once('=').map_or(w, |(k, _)| k) == key)
    })
}

/// `RUSTYNAIL_JIT_TRACE_LIMITS` (7.10).
///
/// **K3a: `TrLimits::default()` is the shipped configuration.** Every knob has the value of its `*_PROCESS` constant, so the tests (which start from
/// the default and name only what they vary) run the configuration that ships. Three knobs differ between a process and the default and are set by
/// [`TrConfig::apply_process_limits`] unless the limits string names them: `sidemin` (a start gate: 0 in tests, the twin programs are tiny), `wgate` (the
/// work-budget start gate: 0 in tests) and `lockstep` (the recorder cross-check: an assert-only oracle that does not change what is recorded; 1 in tests,
/// [`LOCKSTEP_PROCESS`] = 0 in a process). Field docs that say "struct default 0" were written before K3a and mean the old OFF default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrLimits {
    /// Traces per state; reaching it flushes all of them (global penalty).
    pub maxtrace: u32,
    /// IR instructions per recording.
    pub maxrecord: u32,
    /// Snapshots per recording.
    pub maxsnap: u32,
    /// Side traces per root.
    pub maxside: u32,
    /// BUNDLE-O: side traces a family may hold beyond `maxside` (0 = none: the cap is hard). A family that is full defers an exit instead of
    /// blacklisting it: the exit reports again after every `hot_exit` more exits, and once it has been deferred `polywait` times (it has
    /// taken about `polywait * hot_exit` exits, deterministic) it gets a side trace if the family holds fewer than `maxside + polyside`.
    /// Polymorphic call sites (4 receiver classes) need one side trace per case; a flat cap of 16 left them in the interpreter.
    pub polyside: u32,
    /// See `polyside`: deferrals of a capped exit before it may exceed `maxside`.
    pub polywait: u32,
    /// See `polyside`: only an exit that lies in at least `polyframes` inlined frames may exceed `maxside` (0 = any exit): the outcomes of branches
    /// inside the bodies of inlined methods (several receiver classes, each with its own paths).
    pub polyframes: u32,
    /// Inlining depth.
    pub maxdepth: u32,
    /// Recursion unroll.
    pub unroll: u32,
    /// F2a: fewest bytecodes of a side trace (shorter ones are not installed, see `AbortReason::SideShort`); 0 = no
    /// minimum (the default of this struct, used by the tests). **P-F: the process default (`TrConfig::from_env`) is
    /// [`SIDEMIN_PROCESS`]**: a path of fewer bytecodes is cheaper in tier 1 than behind a side-trace hop (docs/JIT_TRACE_REF.md
    /// "P-F decisions and results").
    pub sidemin: u32,
    /// F2b: linking level. 0: the prototype's `InnerLoop` abort; 1 (default): a root recording that reaches an inner
    /// loop head of its own frame that has a root trace ends with a link into it (7.8 "Nests"); 2: also linking exits
    /// from inlined frames (heapsort's `sift`), which measured slower than the status quo (docs/JIT_TRACE_REF.md "F2b").
    pub nest: u32,
    /// F2b: stitching at C calls needs at least this many recorded bytecodes before the call (0: off).
    pub stitchmin: u32,
    /// P-E: backward edges of inlined callees' loops unrolled per recording (0: such a loop aborts `InnerLoop`).
    pub loopunroll: u32,
    /// P-E2: function-entry roots (pc 0 of a loop-free, non-vararg prototype that makes calls is a trace head); 0 = off.
    pub fnroot: u32,
    /// P-M3: native self-recursion (a function-entry root whose first self-call is a native frame push that jumps back
    /// to the trace head); 0 = off (the struct default; the process default is [`RECNAT_PROCESS`] = 1, P-M4). Implies the function-entry heads (like `fnroot`) but not the generic function roots.
    pub recnat: u32,
    /// BUNDLE-E (test knob, default 0, never set by a process default): continuation heads of `recany` also record for a prototype
    /// in which any recording observed a self-call (1), and its recordings may end in native returns (2). Reproduces the loop-recursion
    /// shapes of awfy `queens` that the deterministic work gate does not reach; see "BUNDLE-E decisions and results".
    pub contok: u32,
    /// R2: native recursion for any callee load (`SELF` methods, `GETTABUP` / `GETFIELD` globals and fields, not only `GETUPVAL`),
    /// native `RETURN0`, loops of recursive functions (implies function-entry heads for functions with loops) (0 = as P-M3:
    /// upvalue-loaded callees and `RETURN1` only).
    pub recany: u32,
    /// R2: a side trace entered with virtual frames that needs more stack / spare `CallInfo` nodes than its parent's checks guarantee checks
    /// them itself at its start (0 = the `Compile(side-room)` abort).
    pub siderooms: u32,
    /// R2: a store into an existing hash node that holds nil (a field that was assigned `nil` and is set again) is recorded
    /// (0 = the `Insert` abort).
    pub nilstore: u32,
    /// P-E2: fewest bytecodes a function-entry recording must execute.
    pub fnmin: u32,
    /// M1: `while` / `repeat` loops of inlined callees are unrolled (0 = the `InnerLoop` abort; struct default 0, process default [`LOOPANY_PROCESS`] = 1, guarded by `loopanymax`: unrolled loops of data-dependent trip count exit-storm, see docs).
    pub loopany: u32,
    /// Guard of `loopany`: side-trace attempts at exits inside an inlined callee frame that a family with unrolled callee loops may take before
    /// the family is flushed and its loops (site = callee prototype, loop head pc) are never unrolled again (they abort `InnerLoop` like with
    /// `loopany=0`). A callee loop of constant trip count leaves one such exit, one of data-dependent trip count one per distinct count.
    pub loopanymax: u32,
    /// BUNDLE-P: when the `loopany` guard fires and the family root unrolled no callee loop itself (only side traces did), flush only the side traces that
    /// unrolled (with their subtrees) and keep the root; a root that unrolled (the `heapsort` shape) still flushes the whole family (0 = always the family;
    /// default [`LAPRUNE_PROCESS`] (process and tests)).
    pub laprune: u32,
    /// C36: when the persistent-entry-failure rule fires for a trace that needs spare `CallInfo` nodes (`TR6b`: an inlined native call) and the frame lacks them (tier 1 that
    /// inlines a built-in never makes the real call that would have allocated them), allocate them, reset the head's failure account and keep the trace
    /// (0 = flush and re-record, which compiles to the same code and blacklists the head; default [`CISPARE_PROCESS`] (process and tests)).
    pub cispare: u32,
    /// M1: the least number of recorded steps of such a side trace.
    pub sidestitchmin: u32,
    /// T3: the tier-1 `GETFIELD` / `SELF` / `GETTABUP` templates probe this many `__index` tables (instance -> class -> base class ...) inline (0 = the `jit_get` helper).
    pub t3idx: u32,
    /// BUNDLE-T1I: bit mask of the built-ins the tier-1 `CALL` template runs inline (bit 0 `type`, 1 `math.floor`, 2 `math.abs`, 3 `math.max`,
    /// 4 `math.min`, 5 `math.sqrt`, 6 `string.byte`, 7 `string.len`, 8 `select('#', ...)`, 9 `rawlen`; 0 = the `jit_call` helper for all).
    pub t1inline: u32,
    /// BUNDLE-M C1: the recorder's lockstep cross-check (TR17: the IR so far is run on a model and compared with the VM after every
    /// recorded instruction). 1 = every recording (struct default, tests), 0 = none, K >= 2 = every K-th recording of the process.
    /// `+verify` (and `debug_assertions`) force it on. The recorder's own bookkeeping that the lockstep used to drive (model handles of
    /// new tables, the element budget `Charge`, the `NEWREF` hash image) runs in both modes, so the recorded IR is identical.
    pub lockstep: u32,
    /// DYNMASK: dynamic short-string keys (`t[k]`, `k` an SSA value) use `HREFV` with `HREF_DYN` (the table is the operand; the node-vector size and the node
    /// pointer are read at run time, so a rehash between iterations does not exit) and a load of an absent key reads a static nil node (`HREF_NIL`)
    /// instead of recording the key's identity; a store of an absent dynamic key is the `NEWREF` helper. 0 = the `lsizenode`-guarded form (default [`DYNMASK_PROCESS`], in a process and in the tests).
    pub dynmask: u32,

    /// P2: the ineffective-trace rule counts only the pre-roll exits that leave the trace (no side trace attached to the exit,
    /// or a type-guard exit, which keeps the ping-pong rule of F2a); an exit that continues in a side trace is coverage, not
    /// a failed entry (0 = as before). A root whose tree still grows is not flushed by `Ineffective`; the exit-storm and
    /// hard-backstop rules still end pathological cases (see [`pre_exit_check`]).
    pub ineffcover: u32,
    /// P2: a hot type-guard exit of a rolled root or of a side trace gets a side trace too (0 = it flushes the family, `Rerec`).
    pub typesides: u32,
    /// BUNDLE-P: the deterministic hop rule (counters only): the percent of a root's entries that leave through ONE pre-roll site into a side trace
    /// (a trampoline root) from which the root is ineffective, `HOP_STREAK` checks in a row (0 = off: the struct default; [`HOPTOP_PROCESS`] in a process).
    pub hoptop: u32,
    /// C36: second chances for heads lost to a transient, once per head (bit 0: a head blacklisted by the abort count whose last abort was `LeaveFrame` waits `64 << MAX_ABORTS`
    /// back-edges, a different phase of the counter trigger, and has two more attempts, [`second_retry_due`]; bit 1: the first identical re-record of a head, after a flush
    /// by a hot exit or entry failures, is installed instead of blacklisting the head, [`ident_keep_due`]; 0 = both blacklist; default [`SECOND_PROCESS`] (process and tests)).
    pub second: u32,
    /// BUNDLE-C: a side recording that starts inside inlined frames at an exit whose previous attempt ended `SideShort` gets `sbump` more recursion unrolls
    /// ([`SideStart::ubump`]). The common case: a stitch exit at the recursion limit; a side trace starting AT the limit stitches again at its first call and is too short
    /// by construction, so the exit is blacklisted and storms in tier 1 (`regex_nfa`: two exits of 1 M each). 0 = off; default [`SBUMP_PROCESS`] (process and tests).
    pub sbump: u32,
    /// BUNDLE-D (`ccalls`): the `LIB CCALLS` C-stack check is inline loads and a compare (the helper stays the slow path that answers 0 near the limit) (default [`CCALLS_PROCESS`], in a process and in the tests).
    pub ccalls: u32,
    /// BUNDLE-Q (`t1q`): bit mask of the tier-1 exits removed (1 = `EQ` of objects without a real `__eq` call, 2 = float `MOD`, 4 = BUNDLE-S `__index` Lua function frames, 8 = BUNDLE-S `__newindex` Lua function frames); bit 7 (0x80) = named explicitly (then it also applies
    /// with `RUSTYNAIL_JIT_TRACE=off`, which otherwise stays J4 exactly) (default [`T1Q_PROCESS`]).
    pub t1q: u32,
    /// BUNDLE-S S2 (`amt`): array reads and stores of a table that has a metatable are recorded (a non-nil element is a raw access; a nil element needs the flag guard of `__index` / `__newindex`) (default [`AMT_PROCESS`]).
    pub amt: u32,
    /// BUNDLE-P P2 (`f2ifl`): `F2I_EXACT` of an `FFLOOR` result (already integral or NaN/inf/out of range) is `cvttsd2si; cmp r, 1; jo exit` (an exact `-2^63` result also exits: the guard is conservative there) (default [`F2IFL_PROCESS`]).
    pub f2ifl: u32,
    /// BUNDLE-P P1 (`rearmside`): a rearmed tick also enters a young head (fewer than [`REARM_MIN_ENTRIES`] entries) whose family has this many installed side traces (0 = off; default [`REARMSIDE_PROCESS`]).
    pub rearmside: u32,
    /// C45 (`rearmclean`): 1 = `rearmside` only enters young heads of clean families (default [`REARMCLEAN_PROCESS`]); 0 = the plain rule.
    pub rearmclean: u32,
    /// BUNDLE-D (`armm`): tier 1 continues in compiled code after a Lua `__add/__sub/__mul/__div/__unm/...` metamethod returns (default [`ARMM_PROCESS`], in a process and in the tests).
    pub armm: u32,
    /// BUNDLE-D (`sbump2`): per-exit extra recursion unroll for side attempts that ended `Recursion`/`Depth` inside inlined frames, counter-bounded (default [`SBUMP2_PROCESS`], in a process and in the tests).
    pub sbump2: u32,
    /// BUNDLE-G (`chaindepth`): longest chain of callee-guard side traces (a megamorphic call site: link after link, each attached to the callee-guard exit of the previous
    /// one). A hot callee-guard exit at the end of a chain of this many links gets no further side trace (blacklisted: it stays an exit into tier 1), and the `chainre` re-order
    /// rule is unchanged. 0 = unlimited (struct default, [`CHAINDEPTH_PROCESS`] in a process).
    pub chaindepth: u32,
    /// BUNDLE-F (`stitch0`): 1 = a side recording of a function root with nothing recorded yet does not stitch (no chains of empty side traces; default [`STITCH0_PROCESS`] (process and tests)).
    pub stitch0: u32,
    /// BUNDLE-F (`fibdiet`): bit 0 = the room check after a native push computes from the register-held new function (no reload of the just-stored `ci.func`), bit 1 = fused callstatus test of a native return (default [`FIBDIET_PROCESS`], in a process and in the tests).
    pub fibdiet: u32,
    /// BUNDLE-C: per-site `fnroot`. A function-entry head of the recursion machinery (a probe, `fnroot=0`) that is about to be blacklisted by `FnShort` aborts (the
    /// function was entered from untraced code many times and never recursed) is promoted ONCE to a generic function-entry root of its prototype
    /// (what `fnroot=1` does to every function): the head gets a fresh abort budget and records as `fnroot` would. 0 = off; N = at most N promotions per state; default [`FNPROMO_PROCESS`] (process and tests).
    pub fnpromo: u32,
    /// BUNDLE-C: the promoted head waits `64 << fnpromow` entries before its first generic recording (a short program cannot repay a function root).
    pub fnpromow: u32,
    /// C39: retry rounds of a head blacklisted by two `InnerLoop` aborts in a row (a loop of an inlined callee the recording cannot unroll, or an inner loop of the root
    /// frame without a root trace). Each round waits `64 << (MAX_ABORTS + n)` back-edges (the loops of the callee may have got a trace, the guard of `loopany`
    /// may be satisfied, the callers of the loop may have another shape) and allows ONE attempt; an `InnerLoop` abort of that attempt ends the round. 0 = none
    /// (blacklisted for good, as before); default [`INNERRETRY_PROCESS`] (process and tests).
    pub innerretry: u32,
    /// C38 slice 1: 1 = a SIDE recording that reaches, inside an inlined callee frame, a loop head of that callee with a root trace ends with the frame-linking exit (as `nest=2` does for roots)
    /// instead of unrolling the loop (`loopany`) or aborting `InnerLoop`. Struct default 0, [`SIDENEST_PROCESS`] in a process.
    pub sidenest: u32,
    /// C36: a root with at least `domn` entries since its trace was installed, of which `domtop` percent or more leave through ONE pre-roll site into a side trace for
    /// `HOP_STREAK` checks in a row, was recorded on a rare path of its loop: its family is flushed once (`FlushWhy::Dom`: no penalty counter, no identical-compile
    /// blacklisting) and the root is recorded again (0 = off; default [`DOMTOP_PROCESS`] (process and tests); `hoptop` stays the rule for the all-hopping trampoline).
    pub domtop: u32,
    /// C36: see `domtop`.
    pub domn: u32,
    /// P3: after an entry-failure flush of a root whose entry segment holds a callee identity guard (TR6d), the head is recorded again with callee guards kept out of the entry segment
    /// (0 = off; default [`NCALLEE_PROCESS`] (process and tests)).
    pub ncallee: u32,
    /// P3: a side trace of which `chainre` percent or more of at least `chainn` entries leave through its callee-guard exit into a child (a pass-through link of a
    /// megamorphic call chain), whose chain would visit fewer links per entry in hotness order, gets the child subtree flushed once per exit (the exit records the
    /// currently hot callee first). 0 = off; default [`CHAINRE_PROCESS`] (process and tests).
    pub chainre: u32,
    /// P3: see `chainre`.
    pub chainn: u32,
    /// BUNDLE-B item 2: inline store paths of tier 1 (bit 0 `SETTABLE` with a short-string key into an existing node, bit 1 integer key in the hash part,
    /// bit 2 nil-slot stores; 0 = none).
    pub tset: u32,
    /// H1: such an exit (a family rooted at a function-entry or continuation head) gets its side trace when it has been taken
    /// `fck` times `hot_exit` times (a lower count made short programs record side traces that never repaid their compilation).
    pub fck: u32,
    /// H1: fewest bytecodes of a side trace that starts inside inlined frames and ends at a stitching exit (no link); shorter ones are
    /// `SideShort` (they are one `RETURN` of work).
    pub framemin: u32,
    /// P-B: the allocation storm guard (1 = on): a recording that allocates strings in every iteration aborts `AllocStorm` when the
    /// collector steps about as often as one iteration allocates. Default 0 here (the tests record regardless of the heap's pacing);
    /// the process configuration (`RUSTYNAIL_JIT_TRACE_LIMITS`, `allocguard=0|1`) defaults to 1.
    pub allocguard: u32,
    /// P-G: time-stamp-counter ticks that must pass between the first loop activity of the state (the first sampling tick
    /// of the interpreter's loops, or the first hot firing of a head) and its first recording; a script that ends within
    /// that time never pays for a trace (record, assemble and install cost 100k-300k cycles, more than the first
    /// few hundred microseconds of loop work could gain). A time gate bounds the delay of every longer program by the same
    /// absolute amount whatever its iterations cost (a gate counted in iterations delayed programs with heavy iterations by
    /// up to 25%, and changed the order in which `queens` formed its traces). 0 = record at the first firing (the plain
    /// default, used by the tests); the process configuration (`RUSTYNAIL_JIT_TRACE_LIMITS`, `warm=N`) defaults to
    /// [`DEFAULT_WARM`]. x86-64 only (like the compiler).
    pub warm: u32,
    /// BUNDLE-1: deterministic start gate: back-edges (loop sampling ticks count as `tick_period` each, a head's hot firing as
    /// `hot_loop`) that the state must have run since its first loop activity before the first recording (0 = off; replaces the
    /// time gate `warm`, which makes the trace set of a program vary from run to run). Default 0 here, [`WGATE_PROCESS`] in a process
    /// (unless `warm=` is named: `warm=0` then switches every gate off, `warm=N` selects the time gate as before).
    pub wgate: u32,
    /// BUNDLE-1: the interpreter's tail call (`pre_tail_call`) counts calls of its Lua callee like a call does (the callee is
    /// compiled and entered when hot); before, a function reached only by `return f(x)` was never counted (0 = as before).
    /// Default 0 here, [`TAILHOOK_PROCESS`] in a process.
    pub tailhook: u32,
    /// BUNDLE-1: header-safe peeling: loops with string `TLEN` and read-only `LIB` kinds are peeled (see
    /// `opt_mem::header_safe_ins`). 2 = also `LIB sub`, 3 = also `char`, 4 = also `tostring` (HDRSUB; `TNEW` / `SNEW` / `CONCAT` stay rolled). Default 0 here, [`HDRLIB_PROCESS`] in a process.
    pub hdrlib: u32,
    /// BUNDLE-2: allocation sinking of tables that are given a metatable (`setmetatable({...}, V)`, the object-constructor idiom): the `SETMT` helper
    /// and its guard become a `__gc`-absence flag guard on the metatable, the method lookups on the table are proved absent statically, and
    /// the exit restore sets the metatable of the materialised table (0 = such a table is never sunk). Default 0 here, [`SINKMT_PROCESS`] in a process.
    pub sinkmt: u32,
    /// BUNDLE-5: allocation sinking in side and stitched traces (their writeback is dead by the link target's active locals, or never runs). Default 0 here, [`SINKSIDE_PROCESS`] in a process.
    pub sinkside: u32,
    /// SINK-R: the ineffective-root rule is sink-aware: a root that would be flushed as ineffective while the sunk tables its pre-roll exits name
    /// exist is re-recorded once without sinking those shapes (0 = off, 1 = the shape is demoted globally, 2 = for the root head only).
    /// Default 0 here, [`SINKROOT_PROCESS`] in a process.
    pub sinkroot: u32,
    /// J1-2: an exit that must materialise sunk tables (frameless, not an entry check) is restored by its inline stub, which calls the light
    /// helper `exit::jit_tr_mat` instead of the whole exit handler (0 = the handler). Default 0 here, [`SINKSTUB_PROCESS`] in a process.
    pub sinkstub: u32,
    /// J2-2: per-site deeper inlining: a loop recording that ended at the recursion or depth limit (`Recursion` / `Depth` abort) is
    /// re-recorded once, immediately, with `unroll` and `maxdepth` raised to this value (at most `exit::MAX_FRAMES`), and so are the
    /// side recordings of its family (0 = never, the struct default; process default [`DEEP_PROCESS`]).
    pub deep: u32,
    /// J2-2: most IR instructions of a deep recording (a bigger call tree aborts `TooLong`: the site keeps its penalties).
    pub deepins: u32,
    /// J2-2: a root recorded with the deep limits is exempt from the hop-rate rule of the ineffective-trace check (1; 0 = it is judged like any root).
    pub deephop: u32,
    /// BUNDLE-C: the tier-1 `CALL` template calls light C functions through the lean helper `jit_ccall` (default [`T1CCALL_PROCESS`]).
    pub t1ccall: u32,
    /// BUNDLE-C: `string.format` with a constant format of `%d %x %X %s %c` items (and plain widths) is recorded (`LIB FMT`) (default [`TFMT_PROCESS`]).
    pub tfmt: u32,
    /// BUNDLE-H: library calls on trace (bit 0 recorded calls, bit 1 lean `LIB` entries, bit 2 inline `type` / string compare / `random`) (default [`LIBCALL_PROCESS`]).
    pub libcall: u32,
    /// BUNDLE-N (RETF, docs/JIT_PARKED.md C38): frame-leaving side traces. Bit 0 (value 1): a side recording that reaches the `RETURN` of
    /// its root frame executes `RETF` (the pop is done natively and the trace continues in the caller); bit 1 (value 2): inline re-push
    /// stub; bit 2 (value 4): function-entry roots. Default [`RETF_PROCESS`] (0 = off, byte-identical IR and machine code).
    pub retf: u32,
    /// BUNDLE-N: root `RETURN`s a recording may turn into `RETF` (default [`RETFMAX_PROCESS`]).
    pub retfmax: u32,
    /// BUNDLE-V V2: record upvalue stores of collectable values (default [`UPBAR_PROCESS`]).
    pub upbar: u32,
    /// BUNDLE-H: traversal steps on trace (bit 0: `TNEXT` is header-safe, the loops of `pairs` / `next` peel) (default [`TNEXTINL_PROCESS`]).
    pub tnextinl: u32,
    /// BUNDLE-J: C-entered function roots. A Lua function that a C function calls (`table.sort` comparator, `string.gsub` callback, `pcall`'d function; its caller's frame is a
    /// native frame) is entered into compiled code even when it is small and loop-free, gets a function-entry head (also without a `CALL`) that is a generic function root from the
    /// start (no recursion probe), and is exempt from the short-function minimum `fnmin` (1 = on, 2 = also without the entry back-off monitor; default [`CENTRY_PROCESS`]).
    pub centry: u32,
    /// BUNDLE-J: a C-entered function root whose trace has fewer IR instructions than this is not entered from C (default [`CENTRY_MIN_IR`]).
    pub centrymin: u32,
    /// BUNDLE-J: hot events from C before a registered function is entered (default [`CENTRY_WAIT`]).
    pub centrywait: u32,
    /// BUNDLE-K L1 (`lazyci`): natively pushed self-call frames of the function-entry / continuation traces are lazy (only `func` and the
    /// caller's `savedpc` are stored at the push; a chain is completed at every exit); 0 = off (struct and process default).
    pub lazyci: u32,
    /// BUNDLE-L A (`aidx`): the complemented index `~i` of an array load is an IR value (`NEG k` with a displacement for `k + c`, shared by the loads of the same
    /// index, loop invariant when the index is) instead of `mov; not` in front of every access; 0 = off (default [`AIDX_PROCESS`]).
    pub aidx: u32,
    /// BUNDLE-L B (`bhoist`): the bounds checks `ULT k + c, asize` of a counted `for` loop (induction slot `k` with step 1, loop guard `NE count, 0`) are replaced by three guards at the end
    /// of the pre-roll (`count ULT asize`, `k0 + c ULT asize`, `k0 + count + c ULT asize`); 0 = off (default [`BHOIST_PROCESS`]).
    pub bhoist: u32,
    /// P2-3b: the trace poll is `cmp byte [r15 + disp], 0` when the poll word is within reach of the per-state data (0 = address in a register) (struct default 0;
    /// process default [`POLLR15_PROCESS`] = 0).
    pub pollr15: u32,
    /// P2 bundle 1: extra general registers of the trace register allocator: bit 0 = `rbp` (`V9`, free: only the thunk's frame-pointer chain), bit 1 = `r14` (`V10`, the constants
    /// pointer of tier 1, reloaded from `ci.kcache` before every jump out of the trace) (0 = the 7 / 8 registers of before) (default [`XREGS_PROCESS`]).
    pub xregs: u32,
    /// R1: a stay of compiled code that hands a frame to the interpreter arms the sampling tick (`jit_hooks::rearm_tick`) for the next `rearm` back-edges
    /// (clamped to 64): the tick enters compiled code only at a head with an installed root trace (budgeted per head), without touching warm-up, start gate
    /// or head counters, and restores the formation countdown (0 = the tick counts down a whole period) (default [`REARM_PROCESS`]).
    pub rearm: u32,
}

impl Default for TrLimits {
    fn default() -> TrLimits {
        #[allow(unused_mut)]
        let mut l = TrLimits {
            maxtrace: 1000,
            maxrecord: 4000,
            maxsnap: 500,
            maxside: MAXSIDE_PROCESS,
            polyside: POLYSIDE_PROCESS,
            polywait: POLYWAIT_PROCESS,
            polyframes: POLYFRAMES_PROCESS,
            maxdepth: 4,
            unroll: 2,
            sidemin: 0,
            nest: 1,
            stitchmin: 32,
            loopunroll: 16,
            fnroot: 0,
            recnat: RECNAT_PROCESS,
            contok: 0,
            recany: RECANY_PROCESS,
            siderooms: SIDEROOMS_PROCESS,
            nilstore: NILSTORE_PROCESS,
            fnmin: 16,
            loopany: LOOPANY_PROCESS,
            loopanymax: 4,
            laprune: LAPRUNE_PROCESS,
            cispare: CISPARE_PROCESS,
            sidestitchmin: 4,
            t3idx: T3IDX_PROCESS,
            t1inline: T1INLINE_PROCESS,
            lockstep: 1,
            dynmask: DYNMASK_PROCESS,
            ineffcover: INEFFCOVER_PROCESS,
            typesides: TYPESIDES_PROCESS,
            hoptop: HOPTOP_PROCESS,
            second: SECOND_PROCESS,
            sbump: SBUMP_PROCESS,
            ccalls: CCALLS_PROCESS,
            armm: ARMM_PROCESS,
            f2ifl: F2IFL_PROCESS,
            t1q: T1Q_PROCESS,
            amt: AMT_PROCESS,
            rearmside: REARMSIDE_PROCESS,
            rearmclean: REARMCLEAN_PROCESS,
            sbump2: SBUMP2_PROCESS,
            stitch0: STITCH0_PROCESS,
            chaindepth: CHAINDEPTH_PROCESS,
            fibdiet: FIBDIET_PROCESS,
            fnpromo: FNPROMO_PROCESS,
            fnpromow: FNPROMOW_PROCESS,
            innerretry: INNERRETRY_PROCESS,
            sidenest: SIDENEST_PROCESS,
            domtop: DOMTOP_PROCESS,
            domn: DOMN_PROCESS,
            ncallee: NCALLEE_PROCESS,
            chainre: CHAINRE_PROCESS,
            chainn: CHAINN_PROCESS,
            tset: TSET_PROCESS,
            fck: 64,
            framemin: 8,
            allocguard: 1,
            warm: 0,
            wgate: 0,
            tailhook: TAILHOOK_PROCESS,
            hdrlib: HDRLIB_PROCESS,
            sinkmt: SINKMT_PROCESS,
            sinkside: SINKSIDE_PROCESS,
            sinkroot: SINKROOT_PROCESS,
            sinkstub: SINKSTUB_PROCESS,
            deep: DEEP_PROCESS,
            deepins: 300,
            deephop: 1,
            t1ccall: T1CCALL_PROCESS,
            tfmt: TFMT_PROCESS,
            libcall: LIBCALL_PROCESS,
            retf: RETF_PROCESS,
            retfmax: RETFMAX_PROCESS,
            upbar: UPBAR_PROCESS,
            tnextinl: TNEXTINL_PROCESS,
            centry: CENTRY_PROCESS,
            centrymin: CENTRY_MIN_IR,
            centrywait: CENTRY_WAIT,
            lazyci: LAZYCI_PROCESS,
            aidx: AIDX_PROCESS,
            bhoist: BHOIST_PROCESS,
            pollr15: POLLR15_PROCESS,
            xregs: XREGS_PROCESS,
            rearm: REARM_PROCESS,
        };
        if mutants::active("bb_default_ignored") {
            l.maxside = 64;
            l.tset = 0;
        }
        if mutants::active("polyside_default_ignored") {
            l.polyside = 0;
            l.polywait = 0;
            l.polyframes = 0;
        }
        if mutants::active("recnat_default_ignored") {
            l.recnat = 0;
        }
        if mutants::active("r2_default_ignored") {
            l.recany = 0;
            l.siderooms = 0;
            l.nilstore = 0;
        }
        if mutants::active("loopany_default_ignored") {
            l.loopany = 0;
        }
        if mutants::active("t3_default_ignored") {
            l.t3idx = 0;
        }
        if mutants::active("p2_default_ignored") {
            l.ineffcover = 0;
            l.typesides = 0;
        }
        if mutants::active("b5_default_ignored") {
            l.hoptop = 0;
            l.sinkmt = 0;
            l.sinkside = 0;
        }
        if mutants::active("centry_default_ignored") {
            l.centry = 0;
        }
        if mutants::active("rearmclean_default_ignored") {
            l.rearmclean = 0;
        }
        if mutants::active("tnextinl_default_ignored") {
            l.tnextinl = 0;
        }
        if mutants::active("libcall_default_ignored") {
            l.libcall = 0;
        }
        if mutants::active("b1_default_ignored") {
            l.tailhook = 0;
            l.hdrlib = 0;
        }
        if mutants::active("sinkroot_default_ignored") {
            l.sinkroot = 0;
        }
        if mutants::active("deep_default_ignored") {
            l.deep = 0;
        }
        if mutants::active("loopdiet_default_ignored") {
            l.pollr15 = 0;
        }
        if mutants::active("rearm_default_ignored") {
            l.rearm = 0;
        }
        l
    }
}

impl TrLimits {
    /// Parses `name=value,...`; returns the limits and the offending items.
    pub fn parse(s: &str) -> (TrLimits, Vec<String>) {
        let mut l = TrLimits::default();
        let mut bad = Vec::new();
        for w in s.split([' ', ',']).filter(|w| !w.is_empty()) {
            let Some((k, v)) = w.split_once('=') else {
                bad.push(w.to_string());
                continue;
            };
            let Ok(v) = v.parse::<u32>() else {
                bad.push(w.to_string());
                continue;
            };
            match k {
                "maxtrace" if v > 0 => l.maxtrace = v,
                "maxrecord" if v > 0 => l.maxrecord = v,
                "maxsnap" if v > 0 => l.maxsnap = v,
                "maxside" => l.maxside = v,
                "polyside" => l.polyside = v,
                "polywait" => l.polywait = v,
                "polyframes" => l.polyframes = v,
                "maxdepth" => l.maxdepth = v,
                "unroll" => l.unroll = v,
                "sidemin" => l.sidemin = v,
                "nest" => l.nest = v.min(2),
                "stitchmin" => l.stitchmin = v,
                "loopunroll" => l.loopunroll = v,
                "fnroot" => l.fnroot = v.min(1),
                "recnat" => l.recnat = v.min(1),
                "contok" => l.contok = v.min(2),
                "recany" => l.recany = v.min(1),
                "siderooms" => l.siderooms = v.min(1),
                "nilstore" => l.nilstore = v.min(1),
                "fnmin" => l.fnmin = v,
                "loopany" => l.loopany = v.min(1),
                "loopanymax" => l.loopanymax = v,
                "laprune" => l.laprune = v.min(1),
                "cispare" => l.cispare = v.min(1),
                "sidestitchmin" => l.sidestitchmin = v,
                "t3idx" => l.t3idx = v.min(4),
                "t1inline" => l.t1inline = v.min(1023),
                "lockstep" => l.lockstep = v.min(1_000_000),
                "dynmask" => l.dynmask = v.min(1),
                "ineffcover" => l.ineffcover = v.min(1),
                "typesides" => l.typesides = v.min(1),
                "hoptop" => l.hoptop = v.min(100),
                "second" => l.second = v.min(3),
                "sbump" => l.sbump = v.min(4),
                "ccalls" => l.ccalls = v.min(1),
                "armm" => l.armm = v.min(1),
                "f2ifl" => l.f2ifl = v.min(1),
                "t1q" => l.t1q = v.min(0x7f) | 0x80,
                "amt" => l.amt = v.min(1),
                "rearmside" => l.rearmside = v.min(64),
                "rearmclean" => l.rearmclean = v.min(1),
                "sbump2" => l.sbump2 = v.min(8),
                "stitch0" => l.stitch0 = v.min(1),
                "chaindepth" => l.chaindepth = v.min(256),
                "fibdiet" => l.fibdiet = v.min(15),
                "fnpromo" => l.fnpromo = v.min(64),
                "fnpromow" => l.fnpromow = v.min(12),
                "innerretry" => l.innerretry = v.min(16),
                "sidenest" => l.sidenest = v.min(1),
                "domtop" => l.domtop = v.min(100),
                "domn" => l.domn = v,
                "ncallee" => l.ncallee = v.min(1),
                "chainre" => l.chainre = v.min(100),
                "chainn" => l.chainn = v,
                "tset" => l.tset = v.min(15),
                "fck" => l.fck = v.clamp(1, 1024),
                "framemin" => l.framemin = v,
                "allocguard" => l.allocguard = v.min(1),
                "warm" => l.warm = v,
                "wgate" => l.wgate = v,
                "tailhook" => l.tailhook = v.min(2),
                "hdrlib" => l.hdrlib = v.min(4),
                "sinkmt" => l.sinkmt = v.min(1),
                "sinkside" => l.sinkside = v.min(1),
                "sinkroot" => l.sinkroot = v.min(2),
                "sinkstub" => l.sinkstub = v.min(1),
                "deep" => l.deep = v.min(8),
                "deepins" if v > 0 => l.deepins = v,
                "deephop" => l.deephop = v.min(1),
                "t1ccall" => l.t1ccall = v.min(1),
                "tfmt" => l.tfmt = v.min(1),
                "libcall" => l.libcall = v.min(1023),
                "retf" => l.retf = v.min(15),
                "retfmax" => l.retfmax = v.min(8),
                "upbar" => l.upbar = v.min(1),
                "tnextinl" => l.tnextinl = v.min(1),
                "centry" => l.centry = v.min(2),
                "centrymin" => l.centrymin = v.min(100_000),
                "centrywait" => l.centrywait = v.min(1000),
                "lazyci" => l.lazyci = v.min(1),
                "aidx" => l.aidx = v.min(1),
                "bhoist" => l.bhoist = v.min(1),
                "pollr15" => l.pollr15 = v.min(2),
                "xregs" => l.xregs = v.min(3),
                "rearm" => l.rearm = v.min(64),
                _ => bad.push(w.to_string()),
            }
        }
        (l, bad)
    }
}

/// Everything the trace runtime reads from the environment (`RUSTYNAIL_JIT_TRACE*`, `RUSTYNAIL_JIT_VERBOSE`,
/// `RUSTYNAIL_JIT_DUMP`), per state (tests override it with [`rt::set_trace`]).
#[derive(Clone, Debug)]
pub struct TrConfig {
    /// `RUSTYNAIL_JIT_TRACE`.
    pub mode: TrMode,
    /// Back-edges per head before recording (`RUSTYNAIL_JIT_TRACE_HOT`, first number).
    pub hot_loop: i32,
    /// Exits per exit site before a side trace / re-record (second number).
    pub hot_exit: u32,
    /// `RUSTYNAIL_JIT_TRACE_OPT`.
    pub opts: TraceOpts,
    /// `RUSTYNAIL_JIT_TRACE_LIMITS`.
    pub limits: TrLimits,
    /// `RUSTYNAIL_JIT_VERBOSE`: 0 off, 1 summary, 2 events.
    pub verbose: u8,
    /// F5: `RUSTYNAIL_JIT_TRACE_PREF` (X5): who owns a nest first, the tracer (default) or J4's regions.
    pub pref: TrPref,
    /// `RUSTYNAIL_JIT_DUMP=dir`.
    pub dump: Option<String>,
    /// Problems found while parsing (unknown options, bad numbers); printed at verbose >= 1.
    pub warnings: Vec<String>,
}

impl TrConfig {
    /// Defaults of `mode`: 256/16 back-edges/exits, `Always`: 1/2.
    pub fn for_mode(mode: TrMode) -> TrConfig {
        let (hot_loop, hot_exit) = if mode == TrMode::Always {
            (1, 2)
        } else {
            (256, 16)
        };
        TrConfig {
            mode,
            hot_loop,
            hot_exit,
            opts: TraceOpts::default(),
            limits: TrLimits::default(),
            verbose: 0,
            pref: TrPref::Trace,
            dump: None,
            warnings: Vec::new(),
        }
    }

    /// The pure part of [`TrConfig::from_env`]. `mode`: `RUSTYNAIL_JIT_TRACE` (`off`/`on`/`always`; unset or empty =
    /// `on` since F9, unknown = `off` with a warning), `hot`: `loop[,exit]`, `opt`: `RUSTYNAIL_JIT_TRACE_OPT`, `limits`:
    /// `RUSTYNAIL_JIT_TRACE_LIMITS`, `verbose`: `RUSTYNAIL_JIT_VERBOSE`.
    pub fn parse(
        mode: Option<&str>,
        hot: Option<&str>,
        opt: Option<&str>,
        limits: Option<&str>,
        verbose: Option<&str>,
    ) -> TrConfig {
        let mut warnings = Vec::new();
        let m = match mode.map(|m| m.trim().to_ascii_lowercase()).as_deref() {
            Some("off" | "0" | "false") => TrMode::Off,
            // F9 (D3): unset or empty = `on` in every build that has the trace compiler
            None | Some("" | "on" | "1" | "true") => TrMode::On,
            Some("always") => TrMode::Always,
            Some(o) => {
                warnings.push(format!("unknown RUSTYNAIL_JIT_TRACE value {o:?} (off)"));
                TrMode::Off
            }
        };
        let mut c = TrConfig::for_mode(m);
        if let Some(h) = hot {
            let mut it = h.split(',').map(str::trim);
            if let Some(l) = it.next().filter(|s| !s.is_empty()) {
                match l.parse::<i32>() {
                    Ok(v) if v >= 1 => c.hot_loop = v,
                    _ => warnings.push(format!("bad RUSTYNAIL_JIT_TRACE_HOT loop count {l:?}")),
                }
            }
            if let Some(e) = it.next().filter(|s| !s.is_empty()) {
                match e.parse::<u32>() {
                    Ok(v) if v >= 1 => c.hot_exit = v,
                    _ => warnings.push(format!("bad RUSTYNAIL_JIT_TRACE_HOT exit count {e:?}")),
                }
            }
        }
        if let Some(o) = opt {
            let (opts, bad) = TraceOpts::parse_lenient(o);
            c.opts = opts;
            warnings.extend(bad);
        }
        if let Some(l) = limits {
            let (lim, bad) = TrLimits::parse(l);
            c.limits = lim;
            warnings.extend(bad.into_iter().map(|b| format!("bad trace limit {b:?}")));
        }
        c.verbose = match verbose.map(str::trim) {
            None | Some("" | "0") => 0,
            Some("1") => 1,
            Some(_) => 2,
        };
        c.warnings = warnings;
        c
    }

    /// TR30: Lua 5.4 traces are supported since F7a (the prototype refused them); kept as the single place where a
    /// configuration is validated against the build.
    pub fn checked(self) -> TrConfig {
        self
    }

    /// P-F: the limits that differ between a process (`rnlua`) and the struct defaults the tests use: side traces of fewer than
    /// [`SIDEMIN_PROCESS`] bytecodes are refused unless the limits string names `sidemin` itself; `recnat` is [`RECNAT_PROCESS`]
    /// unless the limits string names `recnat`.
    pub fn apply_process_limits(&mut self, limits_env: Option<&str>) {
        // The struct default of every knob is the shipped value (K3a). Only the three that differ for a process remain here.
        if !names_limit(limits_env, "sidemin") && !mutants::active("sidemin_default_ignored") {
            self.limits.sidemin = SIDEMIN_PROCESS;
        }
        // BUNDLE-M C1: the lockstep cross-check of the recorder (an assert-only oracle: it does not change what is recorded) is on in tests, off in a process
        if !names_limit(limits_env, "lockstep") {
            self.limits.lockstep = LOCKSTEP_PROCESS;
        }
        // BUNDLE-1: a deterministic work budget instead of the time gate (`warm=` or `wgate=` named: as named; `warm=0` switches every gate off)
        if !(names_limit(limits_env, "warm") || names_limit(limits_env, "wgate"))
            && !mutants::active("b1_default_ignored")
        {
            self.limits.wgate = WGATE_PROCESS;
        } else if names_limit(limits_env, "warm") && !names_limit(limits_env, "wgate") {
            self.limits.wgate = 0;
        }
    }

    /// Reads the environment.
    pub fn from_env() -> TrConfig {
        let g = |k: &str| std::env::var(k).ok();
        let limits = g("RUSTYNAIL_JIT_TRACE_LIMITS");
        let mut c = TrConfig::parse(
            g("RUSTYNAIL_JIT_TRACE").as_deref(),
            g("RUSTYNAIL_JIT_TRACE_HOT").as_deref(),
            g("RUSTYNAIL_JIT_TRACE_OPT").as_deref(),
            limits.as_deref(),
            g("RUSTYNAIL_JIT_VERBOSE").as_deref(),
        );
        c.dump = g("RUSTYNAIL_JIT_DUMP").filter(|d| !d.is_empty());
        c.apply_process_limits(g("RUSTYNAIL_JIT_TRACE_LIMITS").as_deref());
        // P-G: ... and the start gate for short scripts
        match g("RUSTYNAIL_JIT_TRACE_PREF").as_deref().map(str::trim) {
            Some("region") => c.pref = TrPref::Region,
            Some("trace" | "") | None => {}
            Some(o) => c.warnings.push(format!(
                "unknown RUSTYNAIL_JIT_TRACE_PREF value {o:?} (trace)"
            )),
        }
        c
    }

    /// The process-wide configuration (read once).
    pub fn process() -> TrConfig {
        static C: OnceLock<TrConfig> = OnceLock::new();
        C.get_or_init(TrConfig::from_env).clone()
    }
}

// ---- ids --------------------------------------------------------------------------------------------------

/// Identity of a recording or trace: a slot of the state's trace table plus a generation counter that is bumped
/// whenever the slot is freed, so an id that outlived its trace is recognisably stale (TR32).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrId {
    /// Slot in the state's trace table (this is the `trace_id` the exit area carries).
    pub slot: u32,
    /// Generation of the slot when the id was handed out.
    pub gen: u32,
}

/// Lifecycle of a loop head (3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadState {
    /// Counting back-edges (initial state; a head that never got hot stays here, "cold").
    Warm,
    /// A recording started at this head is active.
    Recording,
    /// The root trace of the head is installed.
    Traced,
    /// Blacklisted: tier 1 owns the loop for good.
    Black,
}

/// Why a trace is flushed; decides the state the head returns to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlushWhy {
    /// Hot type-guard exit or persistent entry failures: re-record from the observed types (`rerec += 1`).
    Rerec,
    /// H3-G: a sunk allocation site was demoted (its exits materialise it too often): same head transition as `Dep` (re-record, no penalty).
    Unsink,
    /// Q2: most entries leave through a pre-roll guard (the trace never completes an iteration); same head
    /// transition as `Rerec` (`rerec += 1`, blacklist after [`MAX_RERECORD`]).
    Ineffective,
    /// A dependency prototype is being freed.
    Dep,
    /// C36: the root was recorded on a rare path of its loop (most entries hop through one side trace): re-record, no penalty, once per head.
    Dom,
    /// P3 (`chainre`): the child subtree of a pass-through callee-guard exit is flushed so that the exit records the currently hot callee first (side traces only: no head transition).
    Chain,
    /// Code cap or `maxtrace` reached (flush all, global penalty).
    Cap,
    /// The mode was switched off / the state closes.
    Off,
    /// F5 (TR19b(a)): the outer head of the nest went `Black`; the nest goes to J4, its traces are flushed and its heads
    /// become `Black`.
    Nest,
}

/// `exit hot` site classes (3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitSite {
    /// `SLOAD` / `ALOAD` / `ULOAD` typecheck exit.
    TypeGuard,
    /// Branch, bounds, loop-exit, callee-identity guards.
    Other,
    /// P-B: the guard behind a class-A allocation (`NE %alloc, 0` after `TNEW` / `SNEW` / `CONCAT` / `LIB`): taken when the
    /// collector would step right behind the allocation. No side trace is recorded for it (the side trace would allocate, and exit,
    /// again); the exit-storm rule [`alloc_storm`] watches how often it is taken.
    Alloc,
}

// ---- per-head data ----------------------------------------------------------------------------------------

/// `entry_ok` stops counting here (the entry sequence of P5c tests `>= ENTRY_OK_CAP` before incrementing: no wrap).
pub const ENTRY_OK_CAP: u32 = i32::MAX as u32;

/// Counters, the dispatch cell and the penalty state of one loop head. `repr(C)` prefix: machine code addresses
/// `cell`, `ctr`, `pending`, `entry_fail`, `entry_ok` and `entry` ([`head_off`]).
#[repr(C)]
#[derive(Debug)]
pub struct HeadData {
    /// The dispatch cell `cell[H]` (an absolute code address).
    pub cell: Cell<usize>,
    /// Trace counter: back-edges left (decremented by the counting stub; negative = fire).
    pub ctr: Cell<i32>,
    /// Free-peel flag (3.3): the previous entry attempt failed.
    pub pending: Cell<u32>,
    /// Counted entry failures.
    pub entry_fail: Cell<u32>,
    /// Successful entries.
    pub entry_ok: Cell<u32>,
    /// Entry address of the installed root trace (the `+tracemove` stub jumps through it); 0 if none.
    pub entry: Cell<usize>,
    /// The head's pc.
    pub pc: u32,
    /// State.
    pub state: Cell<HeadState>,
    /// Abort count `k` (3.3).
    pub k: Cell<u8>,
    /// Transient refusal count `kt`.
    pub kt: Cell<u8>,
    /// Re-record count.
    pub rerec: Cell<u8>,
    /// F2c: `LeaveLoop` abort count (a loop that is merely short is not a fault: separate, larger budget).
    pub kl: Cell<u8>,
    /// Reason of the last abort with an unchangeable cause (two in a row blacklist).
    pub last_reason: Cell<Option<&'static str>>,
    /// The recording or root trace of the head.
    pub trace: Cell<Option<TrId>>,
    /// `C_H` in the current tier-1 version.
    pub c_addr: Cell<usize>,
    /// `+tracemove` stub in the current tier-1 version (0 = none).
    pub m_addr: Cell<usize>,
    /// `pcl[H]` in the current tier-1 version.
    pub pcl_addr: Cell<usize>,
    /// Q3: shape hash of the last trace of this head flushed `Rerec` / `Ineffective` (0 = none).
    pub flushed_hash: Cell<u64>,
    /// Q3: number of `Ineffective` flushes of this head.
    pub ineff: Cell<u8>,
    /// C36: the head had its `second` abort-count second chance.
    pub second: Cell<bool>,
    /// C39: `innerretry` rounds this head has had.
    pub inner_n: Cell<u8>,
    /// C36: the head had its `second` identical-compile second chance.
    pub ident_kept: Cell<bool>,
    /// C36: the head had its `domtop` re-record.
    pub dom_done: Cell<bool>,
    /// P3 (`ncallee`): the head is recorded with its callee guards out of the entry segment (set by an entry-failure flush of a trace whose entry segment held one; never reset).
    pub ncallee: Cell<bool>,
    /// BUNDLE-L B (`bhoist`): the head is recorded without hoisted bounds guards (set when a root with hoisted guards was flushed as ineffective; never reset).
    pub nohoist: Cell<bool>,
    /// R1 (`rearm`): rearm entries still allowed before a short stay refunds nothing ([`REARM_BUDGET`] consecutive short ones end them for a while).
    pub rearm_budget: Cell<u8>,
    /// R1: refused rearm attempts since the budget ran out.
    pub rearm_wait: Cell<u8>,
    /// R1: the state's compiled-bytecode count (`Stats::insns`) at the last rearm entry into this head (0 = none).
    pub rearm_last: Cell<u64>,
    /// F2a: entry of the **prelude** side trace of the head (the trace of an entry-check exit that is taken more often
    /// than the root is entered): `cell[H]` points at it while the root is `Traced`; its failing entry checks continue
    /// in the root's entry and it ends with a jump through `entry`. 0 if none.
    pub pre: Cell<usize>,
    /// P-M3: address of the inner entry of the installed root trace (past its entry bookkeeping and budget load; 0 if none).
    pub rec_inner: Cell<usize>,
    /// P-M3: the cell a native return / self-call jumps through: the inner entry while the head is `Traced` and nothing else
    /// stands in front of the trace, else the same as `cell`.
    pub rec: Cell<usize>,
    /// BUNDLE-K L1 (`lazyci`): the head belongs to a state whose native recursion is lazy: [`HeadData::rec`] is then the inner entry or 0
    /// (never the plain cell: a jump site of a lazy region leaves the region before it enters anything else).
    pub lazy: Cell<bool>,
    /// J2-2: the site is recorded with the deep limits (`TrLimits::deep`): set once, when a recording of the head ended at the
    /// recursion / depth limit (never reset: the site gets exactly one deeper re-record).
    pub deep: Cell<bool>,
}

/// Byte offsets of the fields machine code touches.
pub mod head_off {
    use super::HeadData;
    use std::mem::offset_of;
    /// `cell`.
    pub const CELL: usize = offset_of!(HeadData, cell);
    /// `ctr: i32`.
    pub const CTR: usize = offset_of!(HeadData, ctr);
    /// `pending: u32`.
    pub const PENDING: usize = offset_of!(HeadData, pending);
    /// `entry_fail: u32`.
    pub const ENTRY_FAIL: usize = offset_of!(HeadData, entry_fail);
    /// `entry_ok: u32`.
    pub const ENTRY_OK: usize = offset_of!(HeadData, entry_ok);
    /// `entry`.
    pub const ENTRY: usize = offset_of!(HeadData, entry);
    /// `rec` (BUNDLE-K L1: the plain `cell` of a head is at `rec - REC + CELL`).
    pub const REC: usize = offset_of!(HeadData, rec);
}

/// Abort count at which a head goes `Black` (3.3).
pub const MAX_ABORTS: u8 = 6;
/// Transient refusal count at which a head goes `Black`.
pub const MAX_TRANSIENT: u8 = 10;
/// F2c: `LeaveLoop` aborts of one head after which it goes `Black` (the wait doubles each time, capped at `64 << 8`).
pub const MAX_LEAVE_LOOP: u8 = 16;
/// P-E2: `FnShort` aborts of one function-entry head after which it goes `Black`.
pub const MAX_FN_SHORT: u8 = 4;
/// Re-records after which a head goes `Black` (`rerec > 3`).
pub const MAX_RERECORD: u8 = 3;
/// `entry_fail` threshold of the prototype's re-record rule.
pub const ENTRY_FAIL_MIN: u32 = 64;

/// What a penalty decision did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Penalty {
    /// The next attempt waits for this many back-edges.
    Wait(i32),
    /// The head is blacklisted.
    Black,
}

impl HeadData {
    fn new(pc: u32) -> HeadData {
        HeadData {
            cell: Cell::new(0),
            ctr: Cell::new(0),
            pending: Cell::new(0),
            entry_fail: Cell::new(0),
            entry_ok: Cell::new(0),
            entry: Cell::new(0),
            pc,
            state: Cell::new(HeadState::Warm),
            k: Cell::new(0),
            kt: Cell::new(0),
            rerec: Cell::new(0),
            kl: Cell::new(0),
            last_reason: Cell::new(None),
            trace: Cell::new(None),
            c_addr: Cell::new(0),
            m_addr: Cell::new(0),
            pcl_addr: Cell::new(0),
            flushed_hash: Cell::new(0),
            ineff: Cell::new(0),
            second: Cell::new(false),
            inner_n: Cell::new(0),
            ident_kept: Cell::new(false),
            dom_done: Cell::new(false),
            ncallee: Cell::new(false),
            nohoist: Cell::new(false),
            rearm_budget: Cell::new(REARM_BUDGET),
            rearm_wait: Cell::new(0),
            rearm_last: Cell::new(0),
            pre: Cell::new(0),
            rec_inner: Cell::new(0),
            rec: Cell::new(0),
            lazy: Cell::new(false),
            deep: Cell::new(false),
        }
    }

    /// Where `cell[H]` must point in the current state (TR20).
    pub fn target(&self) -> usize {
        match self.state.get() {
            HeadState::Warm | HeadState::Recording => self.c_addr.get(),
            HeadState::Traced => {
                if self.pre.get() != 0 {
                    self.pre.get()
                } else if self.m_addr.get() != 0 {
                    self.m_addr.get()
                } else {
                    self.entry.get()
                }
            }
            HeadState::Black => self.pcl_addr.get(),
        }
    }

    fn retarget(&self) {
        self.cell.set(self.target());
        self.set_rec();
    }

    /// P-M3: `rec` follows `cell`, except that a plain `Traced` head is entered at the inner entry.
    fn set_rec(&self) {
        let direct = self.state.get() == HeadState::Traced
            && self.rec_inner.get() != 0
            && self.pre.get() == 0
            && self.m_addr.get() == 0;
        let plain = if self.lazy.get() && !mutants::active("lazy_rec_cell_plain") {
            0
        } else {
            self.cell.get()
        };
        self.rec
            .set(if direct { self.rec_inner.get() } else { plain });
    }

    /// The counter fires after `n` more back-edges (`n <= 1`: at the next one).
    pub fn arm(&self, n: i64) {
        self.ctr
            .set((n - 1).clamp(0, i64::from(i32::MAX / 2)) as i32);
    }

    /// R1: no recording of this head has failed or been flushed yet (no penalty is counting).
    pub fn fresh(&self) -> bool {
        self.k.get() == 0 && self.kt.get() == 0 && self.kl.get() == 0 && self.rerec.get() == 0
    }

    /// R1: `n` back-edges that ran in the interpreter count toward the next firing, but the counter keeps at least `floor` (the
    /// recording must not start in the last iterations of the stay: a tick that lands near the end of a short loop would
    /// make every recording run past the loop's end, `LeaveLoop`).
    pub fn credit(&self, n: i64, floor: i32) {
        self.ctr
            .set((i64::from(self.ctr.get()) - n).max(i64::from(floor)) as i32);
    }

    fn set_state_raw(&self, s: HeadState) {
        self.state.set(s);
    }
}

/// `64 << k` plus the random bits: the penalty before the next attempt (3.3).
pub fn penalty(k: u8, rand4: i32) -> i64 {
    (64i64 << k.min(20)) + i64::from(rand4)
}

/// Per-prototype trace data: boxed fixed-length head table (address-stable, never reallocated, freed with the
/// descriptor; the T3 `T15Data` discipline).
#[derive(Debug)]
pub struct TrProto {
    heads: Box<[HeadData]>,
    /// pc -> head index + 1 (0 = not a head).
    index: Box<[u32]>,
    /// Recordings and traces that depend on this prototype as an inlined callee (TR32).
    deps: RefCell<Vec<TrId>>,
    /// pc -> absolute address of the pc's tier-1 label in the installed code version (0 = dead pc or not yet
    /// installed); rewritten with the head labels by every version install (PIa).
    pcs: Box<[Cell<usize>]>,
    rng: Cell<u32>,
    always: bool,
    hot: i32,
    line: i32,
    /// F5: the loop nests of the prototype in J4's sense, attached by the compile (empty: J4 is off).
    nests: std::cell::OnceCell<Box<[NestG]>>,
    /// F5: per-head J4 data, parallel to `heads`.
    ext: Box<[HeadExt]>,
    /// P-E2: pc 0 is a function-entry head (a root recorded from the first instruction of the function up to the
    /// `RETURN` of its frame), not a loop head.
    fn_head: Cell<bool>,
    /// P-M3 / R2: the continuation heads (the pcs after a `CALL` of a function-entry prototype).
    conts: RefCell<Vec<u32>>,
    /// R2: a native self-call of this prototype was recorded (it recurses, a native return into itself can happen).
    recursive: Cell<bool>,
    /// BUNDLE-K L1: every trace of this prototype is lazy-aware (`TrLimits::lazyci`; fixed when the dispatch table is made).
    lazy: Cell<bool>,
    /// P-G: the block structure of the prototype for the recorder's count model (P-G, always on in the pipeline).
    landing: RefCell<Option<std::rc::Rc<Lowered>>>,
    /// BUNDLE-C: the prototype's function-entry head was promoted to a generic function root (`TrLimits::fnpromo`).
    fnpromo: Cell<bool>,
    /// BUNDLE-J (`centry`): the prototype was first hot when a C function called it; its function-entry head is a C-entered function root (no `fnmin`, no recursion probe).
    cent: Cell<bool>,
    /// BUNDLE-J (`centry`): the C-entered root trace is too light to repay the entry from C (see [`CENTRY_MIN_IR`]): the prototype is not entered from C any more.
    cent_off: Cell<bool>,
}

/// F5: what a head knows about J4 (kept next to `HeadData`, whose layout machine code depends on).
#[derive(Debug, Default)]
struct HeadExt {
    /// J4's promote-counter stub `W_H` in the current tier-1 version (0 = none: the nest is not warm in J4's sense in
    /// that version, or the version has no dispatch for this head).
    w_addr: Cell<usize>,
    /// Index + 1 of the nest this head lies in (0 = none, or J4 is off).
    nest: Cell<u32>,
    /// The head is `Black` and its nest is region-eligible (TR19): `cell[H]` targets `W_H`.
    region_wait: Cell<bool>,
}

/// F5: one J4 nest as the trace policy sees it (TR19): `lo` is the pc of its outer head.
#[derive(Debug)]
struct NestG {
    lo: u32,
    /// J4 owns the nest (promoted) or gave up on it: it is no longer region-eligible.
    closed: Cell<bool>,
}

impl TrProto {
    /// A table for the heads `head_pcs` of a function of `code_len` instructions. `hot` is the re-arm count,
    /// `always` makes the penalty bits deterministic, `seed` seeds them.
    pub fn new(
        code_len: usize,
        head_pcs: &[u32],
        hot: i32,
        always: bool,
        seed: u32,
        line: i32,
    ) -> TrProto {
        let mut index = vec![0u32; if head_pcs.is_empty() { 0 } else { code_len }];
        for (i, &pc) in head_pcs.iter().enumerate() {
            index[pc as usize] = i as u32 + 1;
        }
        TrProto {
            heads: head_pcs.iter().map(|&pc| HeadData::new(pc)).collect(),
            index: index.into_boxed_slice(),
            deps: RefCell::new(Vec::new()),
            pcs: (0..code_len).map(|_| Cell::new(0)).collect(),
            rng: Cell::new(seed | 1),
            always,
            hot: hot.max(1),
            line,
            nests: std::cell::OnceCell::new(),
            ext: head_pcs.iter().map(|_| HeadExt::default()).collect(),
            fn_head: Cell::new(false),
            conts: RefCell::new(Vec::new()),
            recursive: Cell::new(false),
            lazy: Cell::new(false),
            landing: RefCell::new(None),
            fnpromo: Cell::new(false),
            cent: Cell::new(false),
            cent_off: Cell::new(false),
        }
    }

    /// J2-1: is the head at `pc` a function-entry or continuation head (the heads of the recursion machinery)?
    pub fn is_rec_head(&self, pc: u32) -> bool {
        self.is_fn_head(pc) || self.is_cont_head(pc)
    }

    /// P-G: keeps the block structure of the tier-1 lowering that just compiled the prototype for the recorder (the
    /// count model reads only `sig` and the presence of `pc_labels`, identical in every version), without the program.
    pub(crate) fn set_landing(&self, mut l: Lowered) {
        l.prog = Program::default();
        l.points = Vec::new();
        l.resume = Vec::new();
        *self.landing.borrow_mut() = Some(std::rc::Rc::new(l));
    }

    /// P-M3: is the head at `pc` a continuation head (after a self-`CALL`) of a function-entry prototype?
    pub fn is_cont_head(&self, pc: u32) -> bool {
        pc != 0
            && self.fn_head.get()
            && self.head_index(pc).is_some()
            && self.conts.borrow().contains(&pc)
    }

    /// P-E2: is the head at `pc` the function-entry head?
    pub fn is_fn_head(&self, pc: u32) -> bool {
        pc == 0 && self.fn_head.get() && self.head_index(0).is_some()
    }

    /// Re-targets `cell[H]` of head `i` (TR20, TR19b(b)).
    fn retarget(&self, i: usize) {
        let h = &self.heads[i];
        let e = &self.ext[i];
        let t = if h.state.get() == HeadState::Black && e.region_wait.get() && e.w_addr.get() != 0 {
            e.w_addr.get()
        } else {
            h.target()
        };
        h.cell.set(t);
        h.set_rec();
    }

    /// F5 (TR19): attaches J4's nests to the heads. `region_first` (`TRACE_PREF=region`): every head of a nest starts
    /// `Black`, J4 owns the nest first and the tracer only gets it back when J4 gives up.
    pub fn attach_nests(&self, nests: &[(u32, u32)], region_first: bool) {
        let g: Box<[NestG]> = nests
            .iter()
            .map(|&(lo, _)| NestG {
                lo,
                closed: Cell::new(false),
            })
            .collect();
        if self.nests.set(g).is_err() {
            return;
        }
        for (i, h) in self.heads.iter().enumerate() {
            if let Some(n) = nests.iter().position(|&(lo, hi)| (lo..=hi).contains(&h.pc)) {
                self.ext[i].nest.set(n as u32 + 1);
                if region_first {
                    self.set_state(i, HeadState::Black);
                }
            }
        }
    }

    /// F5: recomputes the region-eligibility of every nest (after the labels of a version were installed).
    pub fn sync_all_nests(&self) {
        for n in 0..self.nests.get().map_or(0, |g| g.len()) {
            self.sync_nest(n);
        }
    }

    /// Head `i` changes state; `cell[H]` is re-targeted, and so are the cells of its nest (TR19: a nest is eligible for
    /// J4's promotion when no head is `Recording` or `Traced`, at least one is `Black` and J4 still owns the decision;
    /// then its `Black` heads count in `W_H`, otherwise they fall into `pcl[H]`).
    fn set_state(&self, i: usize, s: HeadState) {
        self.heads[i].set_state_raw(s);
        self.retarget(i);
        let n = self.ext[i].nest.get();
        if n != 0 {
            self.sync_nest(n as usize - 1);
        }
    }

    /// Is the nest `n` (index) region-eligible now (TR19)?
    pub fn nest_eligible(&self, n: usize) -> bool {
        let Some(g) = self.nests.get().and_then(|g| g.get(n)) else {
            return false;
        };
        if g.closed.get() {
            return false;
        }
        let mut black = false;
        for (h, _) in self.members(n) {
            match h.state.get() {
                HeadState::Recording | HeadState::Traced => return false,
                HeadState::Black => black = true,
                HeadState::Warm => {}
            }
        }
        black
    }

    fn sync_nest(&self, n: usize) {
        let elig = self.nest_eligible(n) && !mutants::active("w_cell_skip");
        for (h, i) in self.members(n) {
            let w = elig && h.state.get() == HeadState::Black;
            if mutants::active("region_wait_stale") && self.ext[i].region_wait.get() && !w {
                continue; // MUTANT: a head keeps counting for J4 after the nest stopped being eligible
            }
            self.ext[i].region_wait.set(w);
            if h.state.get() == HeadState::Black {
                self.retarget(i);
            }
        }
    }

    fn members(&self, n: usize) -> impl Iterator<Item = (&HeadData, usize)> + '_ {
        self.heads
            .iter()
            .enumerate()
            .filter(move |(i, _)| self.ext[*i].nest.get() as usize == n + 1)
            .map(|(i, h)| (h, i))
    }

    /// The index of the nest that contains pc `pc` (a head), if J4 nests are attached.
    pub fn nest_of_pc(&self, pc: u32) -> Option<usize> {
        let i = self.head_index(pc)?;
        match self.ext[i].nest.get() {
            0 => None,
            n => Some(n as usize - 1),
        }
    }

    /// J4 promoted nest `n` (its region is installed in the current version) or gave up on it (`promoted == false`):
    /// the nest is closed to the tracer's J4 stub. A promoted nest's heads are all `Black` (the region owns the loop,
    /// TR19); when J4 gave up, `TRACE_PREF=region` hands the heads back to the tracer.
    pub fn close_nest(&self, n: usize, promoted: bool, region_first: bool) {
        let Some(g) = self.nests.get().and_then(|g| g.get(n)) else {
            return;
        };
        g.closed.set(true);
        for (h, i) in self.members(n) {
            if promoted {
                if mutants::active("close_nest_keep_warm") && h.state.get() == HeadState::Warm {
                    continue; // MUTANT: a cold head of a promoted nest stays in the tracer's hands
                }
                if matches!(h.state.get(), HeadState::Warm | HeadState::Black) {
                    h.trace.set(None);
                    self.set_state(i, HeadState::Black);
                }
            } else if region_first && h.state.get() == HeadState::Black && h.c_addr.get() != 0 {
                self.to_warm(i, i64::from(self.hot));
            }
        }
        self.sync_nest(n);
    }

    /// F5 (TR19b(a), the nest rule): head `i` just went `Black`. When it is the outer head of its nest while an inner
    /// head is `Traced` and not linked from an outer trace, returns the traces to flush; the caller flushes them
    /// ([`FlushWhy::Nest`]) and calls [`TrProto::nest_to_black`].
    fn nest_rule_traces(&self, i: usize) -> Vec<TrId> {
        let h = &self.heads[i];
        let n = self.ext[i].nest.get() as usize;
        let Some(g) = n
            .checked_sub(1)
            .and_then(|n| self.nests.get().and_then(|g| g.get(n)))
        else {
            return Vec::new();
        };
        if g.closed.get() || h.pc != g.lo || h.state.get() != HeadState::Black {
            return Vec::new();
        }
        let members: Vec<&HeadData> = self.members(n - 1).map(|(h, _)| h).collect();
        if members
            .iter()
            .any(|m| m.state.get() == HeadState::Recording)
        {
            return Vec::new();
        }
        let traced: Vec<&HeadData> = members
            .into_iter()
            .filter(|m| m.state.get() == HeadState::Traced)
            .collect();
        // "linked from an outer trace": some other traced head lies outside it
        let unlinked = traced.iter().any(|j| !traced.iter().any(|o| o.pc < j.pc));
        if !unlinked {
            return Vec::new();
        }
        traced.iter().filter_map(|m| m.trace.get()).collect()
    }

    /// The nest of head `i` goes to J4 as a whole: every head still `Warm` or `Traced`-and-flushed becomes `Black`.
    fn nest_to_black(&self, i: usize) {
        let Some(n) = self.ext[i].nest.get().checked_sub(1) else {
            return;
        };
        for (h, j) in self.members(n as usize) {
            if h.state.get() == HeadState::Warm {
                h.trace.set(None);
                self.set_state(j, HeadState::Black);
            }
        }
    }

    /// Bytes the table keeps (charged with the descriptor).
    pub fn bytes_for(code_len: usize, heads: usize) -> usize {
        std::mem::size_of::<TrProto>()
            + std::mem::size_of::<usize>() // the `JitProto::tr` pointer (see `rt::descriptor_bytes`)
            + heads * (std::mem::size_of::<HeadData>() + std::mem::size_of::<HeadExt>() + std::mem::size_of::<NestG>())
            + code_len * (4 + std::mem::size_of::<usize>())
            + 4 * std::mem::size_of::<TrId>()
    }

    /// Bytes of this table.
    pub fn bytes(&self) -> usize {
        TrProto::bytes_for(self.pcs.len(), self.heads.len())
    }

    /// The heads, in pc order.
    pub fn heads(&self) -> &[HeadData] {
        &self.heads
    }

    /// Head `i`.
    pub fn head(&self, i: usize) -> &HeadData {
        &self.heads[i]
    }

    /// Index of the head at `pc`.
    pub fn head_index(&self, pc: u32) -> Option<usize> {
        match self.index.get(pc as usize) {
            Some(&v) if v != 0 => Some(v as usize - 1),
            _ => None,
        }
    }

    /// The absolute address of the tier-1 label of `pc` in the installed code version (`None`: a dead pc, a pc out of
    /// range, or no version installed yet). Exits land here (`exit::Resolver::land_addr`, both `Land` kinds use the
    /// pc label; the `Mid` pre-payment is the exit's `land` operand). Rewritten with the head labels by every version
    /// install (TR19b(c)): read it when the exits are built, never cache it across a recompile.
    pub fn pc_addr(&self, pc: u32) -> Option<u64> {
        match self.pcs.get(pc as usize)?.get() {
            0 => None,
            a => Some(a as u64),
        }
    }

    /// Sets the pc label addresses of the installed version (`pc_map` offsets + 1, 0 = dead, relative to `base`).
    pub(crate) fn set_pc_map(&self, base: usize, map: &[u32]) {
        for (c, &o) in self.pcs.iter().zip(map) {
            c.set(if o == 0 { 0 } else { base + o as usize - 1 });
        }
    }

    /// Defined line of the prototype (for logs).
    pub fn line(&self) -> i32 {
        self.line
    }

    /// Sets the current tier-1 version's label addresses of head `i` and re-targets its cell (TR19b(c): the
    /// version-install path calls this for every head in the step that installs the version; `Traced` cells keep
    /// pointing at their trace unless the `+tracemove` stub changes).
    pub fn set_version(&self, i: usize, c: usize, m: usize, pcl: usize) {
        self.set_version_w(i, c, m, pcl, 0);
    }

    /// [`TrProto::set_version`] with J4's promote-counter stub `w` of the version (F5).
    pub fn set_version_w(&self, i: usize, c: usize, m: usize, pcl: usize, w: usize) {
        let h = &self.heads[i];
        if mutants::active("cell_stale_version") && h.pcl_addr.get() != 0 {
            return; // MUTANT: a later version install leaves the cell on the labels of the first version
        }
        h.c_addr.set(c);
        h.m_addr.set(m);
        h.pcl_addr.set(pcl);
        self.ext[i].w_addr.set(w);
        // (a `Traced` head's target is its trace, or the `+tracemove` stub of the new version: `target` knows)
        self.retarget(i);
    }

    /// R1: what one sampling tick of the interpreter adds to the counter of a head it enters tier 1 at: an eighth of the threshold.
    pub fn tick_credit(&self) -> i64 {
        i64::from((self.hot / 8).max(1))
    }

    /// R1: does any head of the prototype hold an installed root trace?
    pub fn any_traced(&self) -> bool {
        self.heads
            .iter()
            .any(|h| h.state.get() == HeadState::Traced)
    }

    /// Re-arms head `i` to the normal threshold.
    pub fn rearm(&self, i: usize) {
        self.heads[i].arm(i64::from(self.hot));
    }

    fn rand4(&self, pc: u32) -> i32 {
        if self.always {
            // deterministic: a hash of (seed, pc)
            let h = (self.rng.get() ^ pc.wrapping_mul(0x9E37_79B1)).wrapping_mul(0x85EB_CA6B);
            ((h >> 13) & 15) as i32
        } else {
            let mut x = self.rng.get();
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.rng.set(x);
            (x & 15) as i32
        }
    }

    /// Blacklists head `i` (its trace, if any, must already be flushed).
    pub fn blacklist(&self, i: usize) {
        self.set_state(i, HeadState::Black);
    }

    /// Head `i` is `Warm` and idle again with a counter of `n` back-edges.
    fn to_warm(&self, i: usize, n: i64) {
        let h = &self.heads[i];
        h.trace.set(None);
        h.entry.set(0);
        h.rec_inner.set(0);
        h.pre.set(0);
        h.entry_fail.set(0);
        h.entry_ok.set(0);
        h.pending.set(0);
        self.set_state(i, HeadState::Warm);
        h.arm(n);
    }

    /// A recording started at head `i` was aborted. A cause that cannot change (`unchangeable`: NYI opcode, C
    /// function, vararg callee, ...) twice in a row blacklists; otherwise the penalty is `2^k * 64 + rand4` and
    /// `k >= 6` blacklists (TR21).
    pub fn on_abort(&self, i: usize, reason: &'static str, unchangeable: bool) -> Penalty {
        let h = &self.heads[i];
        h.trace.set(None);
        if reason == "LeaveLoop" || reason == "FnShort" {
            // F2c: the recording ran past the end of a short loop (data, not a fault of the head): own budget, own
            // backoff; `k` and `last_reason` are untouched. The phase moves by `kl * 3` back-edges (plus `rand4`) so a
            // loop of 3-4 iterations per entry is not always sampled at the same iteration.
            let kl = h.kl.get().saturating_add(1);
            h.kl.set(kl);
            // (P-E2: a function-entry recording that ends short is a leaf-call sample; four in a row and the head is not
            // worth the recordings, each of which costs a lowering for the landing labels and an interpreter run)
            if kl
                >= if reason == "FnShort" && !mutants::active("fn_short_budget_ignored") {
                    MAX_FN_SHORT
                } else {
                    MAX_LEAVE_LOOP
                }
            {
                self.set_state(i, HeadState::Black);
                return Penalty::Black;
            }
            let p = penalty((kl - 1).min(8), self.rand4(h.pc)) + i64::from(kl) * 3;
            self.to_warm_keep(i, p);
            return Penalty::Wait(p as i32);
        }
        let k = h.k.get();
        let repeat = unchangeable && h.last_reason.get() == Some(reason);
        h.last_reason.set(unchangeable.then_some(reason));
        h.k.set(k.saturating_add(1));
        if repeat || k + 1 >= MAX_ABORTS {
            self.set_state(i, HeadState::Black);
            return Penalty::Black;
        }
        let p = penalty(k, self.rand4(h.pc));
        self.to_warm_keep(i, p);
        Penalty::Wait(p as i32)
    }

    /// C36: head `i` was just blacklisted by the abort count (changeable causes): once per head it returns to `Warm` with a long wait and two more attempts.
    pub fn second_chance(&self, i: usize) -> Option<Penalty> {
        let h = &self.heads[i];
        if h.second.get() && !mutants::active("second_retry_repeats") {
            return None;
        }
        h.second.set(true);
        h.k.set(MAX_ABORTS - 2);
        h.last_reason.set(None);
        let p = penalty(MAX_ABORTS, self.rand4(h.pc));
        self.to_warm_keep(i, p);
        Some(Penalty::Wait(p as i32))
    }

    /// BUNDLE-C: head `i` (a function-entry probe) was just blacklisted by `FnShort` aborts: once per prototype it returns to `Warm` with a fresh abort budget as a generic
    /// function root (see [`TrLimits::fnpromo`]).
    pub fn promote_fn(&self, i: usize, w: u32) -> Option<Penalty> {
        let h = &self.heads[i];
        if mutants::active("fnpromo_never")
            || (self.fnpromo.get() && !mutants::active("fnpromo_repeats"))
        {
            return None;
        }
        self.fnpromo.set(true);
        h.k.set(0);
        h.last_reason.set(None);
        let p = penalty(w as u8, self.rand4(h.pc));
        self.to_warm_keep(i, p);
        Some(Penalty::Wait(p as i32))
    }

    /// BUNDLE-C: is the prototype's function-entry head promoted to a generic function root?
    pub fn fn_promoted(&self) -> bool {
        self.fnpromo.get()
    }

    /// BUNDLE-J: is the prototype's function-entry head a C-entered function root (`centry`)?
    pub fn is_cent(&self) -> bool {
        self.cent.get()
    }

    /// BUNDLE-J: the C-entered root trace of this prototype is too light: calls from C stay in the interpreter.
    pub fn cent_off(&self) -> bool {
        self.cent_off.get()
    }

    /// C39: head `i` was just blacklisted by `InnerLoop` aborts: round `n + 1` of `innerretry` returns it to `Warm` with a wait of `64 << (MAX_ABORTS + n)` back-edges and one attempt.
    pub fn inner_chance(&self, i: usize, max: u32) -> Option<Penalty> {
        let h = &self.heads[i];
        let n = h.inner_n.get();
        if !inner_retry_due(max, true, "InnerLoop", n) {
            return None;
        }
        h.inner_n.set(n + 1);
        h.k.set(MAX_ABORTS - 2);
        h.last_reason.set(Some("InnerLoop")); // the next `InnerLoop` abort ends the round
        let p = penalty(MAX_ABORTS + n + INNERRETRY_WAIT_SHIFT, self.rand4(h.pc));
        self.to_warm_keep(i, p);
        Some(Penalty::Wait(p as i32))
    }

    /// J2-2: head `i` is recorded with the deep limits from now on.
    pub fn head_deep(&self, i: usize) -> bool {
        self.heads[i].deep.get()
    }

    /// J2-2: the recording of head `i` ended at the recursion / depth limit: if the site has not had its deeper re-record yet, it gets it
    /// now (the head fires at the next back-edge, no penalty counter changes). False: the site had it already, the normal penalty applies.
    pub fn on_deep(&self, i: usize) -> bool {
        let h = &self.heads[i];
        if h.deep.get() && !mutants::active("deep_every_time") {
            return false;
        }
        h.deep.set(true);
        h.trace.set(None);
        self.to_warm_keep(i, 1);
        true
    }

    /// A recording of head `i` was dropped without fault of the head (a dependency or the root was freed, the
    /// recorder disappeared, traces were switched off): no penalty counter changes, the head waits a normal period.
    pub fn on_dropped(&self, i: usize) {
        self.heads[i].trace.set(None);
        self.to_warm_keep(i, i64::from(self.hot));
    }

    /// Like `to_warm` but keeps the entry counters (a failed recording does not touch a trace's statistics).
    fn to_warm_keep(&self, i: usize, n: i64) {
        let h = &self.heads[i];
        self.set_state(i, HeadState::Warm);
        h.arm(n);
    }

    /// The compile of a recording was refused for a transient reason (code cap, memory limit): penalty
    /// `2^kt * 64 + rand4`, `kt` separate from `k`; at `kt = 10` blacklist.
    pub fn on_transient(&self, i: usize) -> Penalty {
        let h = &self.heads[i];
        h.trace.set(None);
        let kt = h.kt.get();
        h.kt.set(kt.saturating_add(1));
        if kt + 1 >= MAX_TRANSIENT {
            self.set_state(i, HeadState::Black);
            return Penalty::Black;
        }
        let p = penalty(kt, self.rand4(h.pc));
        self.to_warm_keep(i, p);
        Penalty::Wait(p as i32)
    }

    /// `Unstable` (TR24c) or a hot type-guard exit / persistent entry failure: no penalty, `rerec += 1`, the counter
    /// fires at the next back-edge (so the next recording starts from the types the finished iteration produced);
    /// `rerec > 3` blacklists.
    pub fn on_rerecord(&self, i: usize) -> Penalty {
        let h = &self.heads[i];
        h.trace.set(None);
        let r = h.rerec.get().saturating_add(1);
        h.rerec.set(r);
        if r > MAX_RERECORD {
            self.set_state(i, HeadState::Black);
            return Penalty::Black;
        }
        self.to_warm(i, 1);
        Penalty::Wait(1)
    }

    /// Q3: an `Ineffective` flush of head `i`. The second one of the same head blacklists at once (the first
    /// re-record did not cure it); otherwise the `Rerec` transition.
    pub fn on_ineffective(&self, i: usize) -> Penalty {
        let h = &self.heads[i];
        let n = h.ineff.get().saturating_add(1);
        h.ineff.set(n);
        // C39 follow-up: a trace installed after retry rounds of `innerretry` (`inner_n > 0`) that turns out ineffective ends the head
        // for good (no re-record): the retried loop is the data-dependent inner loop the first rounds could not record, and its
        // re-record repeated the failure at 2x the code and 40k exits (apps/expr_lang +30 % instructions).
        if n >= 2 || h.inner_n.get() > 0 {
            h.trace.set(None);
            h.rerec.set(h.rerec.get().saturating_add(1));
            self.set_state(i, HeadState::Black);
            return Penalty::Black;
        }
        self.on_rerecord(i)
    }

    /// Q3: a new compile of head `i` is byte-identical to the trace flushed last: re-installing it would repeat the
    /// failure. The head is blacklisted.
    pub fn on_identical(&self, i: usize) -> Penalty {
        let h = &self.heads[i];
        h.trace.set(None);
        self.set_state(i, HeadState::Black);
        Penalty::Black
    }

    /// A root entry check failed (the failure path of the trace; the machine code of P5b does exactly this with
    /// branch-free memory updates): counts the failure only when the previous entry attempt of the head failed too
    /// (free peel). Returns whether the threshold is reached (`entry_fail >= 64 && entry_fail * 4 > entry_ok`).
    pub fn note_entry_fail(&self, i: usize) -> bool {
        let h = &self.heads[i];
        h.entry_fail
            .set(h.entry_fail.get().saturating_add(h.pending.get()));
        h.pending.set(1);
        self.entry_fail_hot(i)
    }

    /// A successful entry.
    pub fn note_entry_ok(&self, i: usize) {
        let h = &self.heads[i];
        h.pending.set(0);
        // saturates at `ENTRY_OK_CAP` like the machine code of the entry sequence
        if h.entry_ok.get() < ENTRY_OK_CAP {
            h.entry_ok.set(h.entry_ok.get() + 1);
        }
    }

    /// Is the persistent-entry-failure threshold reached?
    pub fn entry_fail_hot(&self, i: usize) -> bool {
        let h = &self.heads[i];
        let (f, ok) = (h.entry_fail.get(), h.entry_ok.get());
        f >= ENTRY_FAIL_MIN && u64::from(f) * 4 > u64::from(ok)
    }
}

// ---- per-state cache --------------------------------------------------------------------------------------

/// Hook called when a recording must stop because a prototype of its dependency set (or its root) is being freed
/// (TR32). The recorder reports `Abort::DepFreed` the next time it runs; it cannot touch the `State` (the collector is
/// running), so the hook receives only the id. [`rec_aborted`] also answers the question.
pub type DepFreedFn = fn(TrId);

/// Starts a recording at `pc` of the Lua frame `ci` (P3a): arm the VM hook (`jit_hooks::rec_arm`) and remember `id`.
/// False = refused (nothing armed).
pub type StartFn = fn(&mut State, Ci, TrId, u32) -> bool;

static DEFAULT_START: OnceLock<StartFn> = OnceLock::new();

/// Installs the process-wide recorder entry (first registration wins); P3a calls it from its `register`.
pub fn install_recorder(f: StartFn) -> bool {
    DEFAULT_START.set(f).is_ok()
}

/// Counters for `RUSTYNAIL_JIT_VERBOSE=1`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrStats {
    /// Recordings started.
    pub started: u64,
    /// Traces installed.
    pub installed: u64,
    /// Recordings aborted, by reason.
    pub aborted: BTreeMap<&'static str, u64>,
    /// Traces flushed.
    pub flushes: u64,
    /// Re-record decisions.
    pub rerec: u64,
    /// Traces flushed by the ineffective-trace rule (a subset of `rerec`).
    pub ineffective: u64,
    /// `loopany` guard: families flushed with their unrolled callee loops blocked.
    pub la_blocked: u64,
    /// C36: heads given a second chance by `second`.
    pub retried: u64,
    /// C39: retry rounds started by `innerretry`.
    pub inner_retried: u64,
    /// BUNDLE-C: function-entry heads promoted to generic function roots by `fnpromo`.
    pub fn_promoted: u64,
    /// BUNDLE-J: prototypes registered as C-entered (`centry`).
    pub c_entered: u64,
    /// BUNDLE-J: C-entered function roots whose trace was too light (`CENTRY_MIN_IR`).
    pub c_light: u64,
    /// BUNDLE-J: calls from C that entered compiled code (`HotCall::Enter` for a C-entered call).
    pub c_enters: u64,
    /// C36: identical re-records installed by `second` bit 1.
    pub ident_kept: u64,
    /// C36: roots re-recorded by the `domtop` rule.
    pub dom_flushes: u64,
    /// P3: chain re-orders (child subtrees flushed by [`chain_check`]).
    pub chain_flushes: u64,
    /// P3: chain checks made ([`chain_check`]).
    pub chain_checks: u64,
    /// BUNDLE-G (`chaindepth`): hot callee-guard exits at the end of a full chain that were blacklisted instead of getting another link.
    pub chain_deep: u64,
    /// P3: heads re-recorded with their callee guards out of the entry segment (`ncallee`).
    pub ncallee_heads: u64,
    /// C36: persistent entry failures repaired by allocating spare `CallInfo` nodes (`cispare`).
    pub ci_repaired: u64,
    /// `loopany` guard (`laprune`): side traces flushed alone (the root stayed).
    pub la_pruned: u64,
    /// `loopany` guard: guard firings that flushed the whole family.
    pub la_family: u64,
    /// `loopany` guard (`laprune`): pruned side traces that were the trace whose exit raised the guard (flushed under their own exit handler).
    pub la_self: u64,
    /// Heads blacklisted.
    pub blacklisted: u64,
    /// Transient refusals.
    pub refused: u64,
    /// Hot type-guard exits.
    pub exit_type_hot: u64,
    /// Hot other exits (counted only in the prototype).
    pub exit_other_hot: u64,
    /// Flushes that were deferred because an exit handler was running (TR18).
    pub deferred: u64,
    /// Entries into traces (summed from the heads of freed prototypes and, at report time, live ones).
    pub entered: u64,
    /// `+tracemove` stack moves.
    pub moves: u64,
    /// Exits taken by traces that were flushed (PI; live traces are added at report time).
    pub exits: u64,
    /// F2a: side recordings started.
    pub side_started: u64,
    /// C38 slice 1: side traces installed that end with a frame-linking exit into a callee loop trace (`sidenest`).
    pub side_frame_links: u64,
    /// F2a: side traces installed.
    pub side_installed: u64,
    /// F2a: side recordings that failed (aborted, refused, compile failed).
    pub side_failed: u64,
    /// F2a: side traces installed as the prelude of their head.
    pub side_preludes: u64,
    /// H3-A: exit-handler calls for an exit that is blacklisted (it will never get a side trace).
    pub black_handled: u64,
    /// F2a: side traces flushed (`flushes` counts the roots only).
    pub side_flushes: u64,
    /// F2a: side recordings aborted, by reason.
    pub side_aborted: BTreeMap<&'static str, u64>,
    /// F2b: root traces installed that end with a link into an inner loop's trace (nest roots).
    pub nest_installed: u64,
    /// F2b: traces installed that end with a linking exit (inlined frames materialised, then the inner trace).
    pub frame_links: u64,
    /// F2b: traces installed that end with a stitching exit at a C call.
    pub stitches: u64,
    /// J1-1: `HREFK`+`HLOAD` pairs whose tag checks were fused into one compare (summed over the installed traces).
    pub hk_fused: u64,
    /// J1-1: of those, the pairs whose two guards exit through different snapshots.
    pub hk_split: u64,
    /// J2-2: sites re-recorded with the deep limits.
    pub deep_sites: u64,
    /// J2-2: recordings (root and side) started with the deep limits.
    pub deep_recs: u64,
    /// F5: nests handed to J4 by the nest rule (TR19b(a)).
    pub nest_rule: u64,
    /// F5: times the nest rule was withheld because an inner trace inlines calls.
    pub nest_rule_kept: u64,
    /// R1: rearm ticks armed by a hand-off to the interpreter.
    pub rearm_arms: u64,
    /// R1: rearm ticks that entered a traced head.
    pub rearm_enters: u64,
    /// R1: rearm ticks that found no traced head (or a head over its budget).
    pub rearm_misses: u64,
    /// R1: rearmed ticks that found the formation countdown restored (below a whole period: the arming did not reset it).
    pub rearm_restored: u64,
    /// C45: rearmed ticks that met a young head with enough side traces whose family was refused for a failed side recording / for an unrolled callee loop (`rearmclean`).
    pub rearm_unclean: [u64; 2],
}

/// F2a: a side recording or trace hangs off exit `exit` (a snapshot index) of trace `parent` (7.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SideLink {
    parent: TrId,
    exit: u32,
    /// A prelude trace (see [`HeadData::pre`]): reached from the head's dispatch cell, not from the parent's exit cell.
    prelude: bool,
}

/// F2a: what an exit of a trace may still become (7.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SideSt {
    /// No side trace; one may be recorded when the exit gets hot.
    Free,
    /// This side trace is attached (`exit_cell[exit]` points at its entry).
    Has(TrId),
    /// Failed [`MAX_SIDE_FAILS`] times, or the family is full: the exit keeps landing in tier 1.
    Black,
    /// F2b: the last recording reached a loop head that has no root trace yet (`SideUntraced`): the exit waits until a
    /// root trace is installed in this prototype (then it is `Free` again with a fresh counter), instead of retrying
    /// every `hot_exit` exits.
    Wait,
}

/// Failed side recordings of one exit after which it is blacklisted (7.9: "the exit failed 4 times").
pub const MAX_SIDE_FAILS: u8 = 4;

struct Rec {
    root: usize,
    head: usize,
    deps: Vec<usize>,
    dep_freed: bool,
    charged: usize,
    exits: Option<Box<[Cell<u32>]>>,
    /// F2a: the exit cells of the trace being compiled ([`alloc_exit_cells`]) and their initial values.
    cells: Option<Box<[Cell<usize>]>>,
    cells_init: Vec<usize>,
    /// F2a: `Some` for a side recording.
    side: Option<SideLink>,
    no: u32,
}

struct Trace {
    root: usize,
    head: usize,
    deps: Vec<usize>,
    block: Option<CodeBlock>,
    own: usize,
    meta: usize,
    exits: Box<[Cell<u32>]>,
    /// Q3: [`shape_hash`] of the compile.
    hash: u64,
    /// Exit metadata of the exit handler ([`attach_exits`]); freed with the trace (retire / grave). The handler clones
    /// the `Arc`, so a flush requested inside it cannot free what it is reading.
    xmeta: Option<Arc<ExitMeta>>,
    no: u32,
    /// F2a: the entry address of the trace.
    entry: usize,
    /// F2a (TR8): `cells[k]` is where exit `k` continues after its state is restored (tier-1 label, or a side trace).
    cells: Box<[Cell<usize>]>,
    /// F2a: the values the cells started with (what a cell goes back to when its side trace is flushed alone).
    cells_init: Vec<usize>,
    /// F2a: `Some` for a side trace.
    side: Option<SideLink>,
    /// F2a: the root trace of the family (itself for a root).
    family: TrId,
    /// F2a: the side traces attached to this trace's exits.
    kids: Vec<TrId>,
    /// F2a: per exit: side-trace state and failed recordings.
    exit_st: Vec<SideSt>,
    fails: Vec<u8>,
    /// N3a: per exit: the last side recording that used `RETF` failed; the next one is recorded without (the exit keeps the trace that ends before the `RETURN`).
    noretf: Vec<bool>,
    /// F2a: the reason of the last failed side recording of each exit if its cause cannot change (two in a row
    /// blacklist the exit, as for a loop head, 3.3).
    last: Vec<Option<&'static str>>,
    /// F2a: `entry_ok` of the head when an entry-check exit first got hot and the prelude decision was deferred (see
    /// `side_attempt`).
    mark: Vec<Option<u32>>,
    /// F2a (roots): side traces of the whole family that exist now (`maxside`).
    nside: u32,
    /// P-M3: a native-return trace: its exits through the empty start-of-trace snapshot (the caller-site guard, the poll, the
    /// budget) never get side traces (the guard fails for every call from outside the recursion; a side trace would start with
    /// the same guard).
    no_start_side: bool,
    /// BUNDLE-O O0 (diagnostic, `RUSTYNAIL_JIT_EXITDUMP`): guard classes per snapshot ([`exit::guard_kinds`]).
    gk: Vec<u16>,
    /// `loopany` guard: the callee loops (prototype address, head pc) this trace unrolled.
    la: Vec<(usize, usize)>,
    /// `loopany` guard (family root): side attempts inside callee frames so far.
    la_sides: u32,
    /// C45 (family root): failed side recordings (`side_failed` with `fault`) of any trace of the family so far; never decremented (a flushed side trace's failures count).
    side_faults: u32,
    /// C45 (family root): a trace of the family unrolled a callee loop (`la` non-empty) at some time.
    la_any: bool,
    /// BUNDLE-O: per exit, how often a full family deferred it (see `TrLimits::polyside`).
    defer: Vec<u8>,
    /// P3: per snapshot, whether the exit counter counts twice per exit (`Asm::dbl`).
    dbl: Vec<bool>,
    /// P3 (`ncallee`): the entry segment of this root holds a callee identity guard (`EQREF` on an empty-map snapshot).
    entry_callee: bool,
    /// BUNDLE-L B: the trace holds hoisted bounds guards ([`TraceIr::hoisted`]).
    hoisted: bool,
    /// P3 (`chainre`): per exit, the child subtree of the exit was flushed for re-ordering ([`chain_check`]).
    chain_done: Vec<u8>,
    /// P3 (family roots): chain re-orders of the whole family so far.
    chain_n: u32,
    /// P3: per exit, the exits counted before the counter was re-based by [`chain_check`] (one count per exit).
    chain_off: Vec<u64>,
}

// (a slot of the trace table: boxing `Trace` would cost an indirection in the exit handler's lookups for nothing)
#[allow(clippy::large_enum_variant)]
enum Body {
    Free,
    Rec(Rec),
    Trace(Trace),
}

struct Slot {
    gen: u32,
    body: Body,
}

/// BUNDLE-P: number of clock reads (time-stamp counter) made by trace policy decisions in this process (the warm gate `warm`). Zero under the default limits: a test asserts it.
static CLOCK_READS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// BUNDLE-P: see [`CLOCK_READS`].
pub fn clock_reads() -> u64 {
    CLOCK_READS.load(std::sync::atomic::Ordering::Relaxed)
}

/// BUNDLE-P: a policy decision reads a clock (counted, see [`clock_reads`]).
pub(crate) fn note_clock_read() {
    CLOCK_READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// The time-stamp counter (x86-64; the compiler exists for that target only).
#[inline]
fn tsc() -> u64 {
    note_clock_read();
    #[cfg(miri)]
    {
        0 // (Miri cannot call the intrinsic)
    }
    #[cfg(not(miri))]
    // SAFETY: `rdtsc` has no preconditions.
    unsafe {
        std::arch::x86_64::_rdtsc()
    }
}

/// BUNDLE-1: bytecodes of the loop whose head is `head` (head up to its backward edge; 8 when no edge to `head` is found): the weight
/// of one iteration in the start gate's count of executed work (a back-edge of a 3-instruction loop costs far less than one of a
/// 60-instruction body, and iteration counts alone made the delay of the first recording vary from 1x to 100x in time).
pub(crate) fn loop_len(code: &[u32], head: usize) -> u64 {
    use rn_core::instruction::Instruction;
    use rn_core::ver::active::OpCode as O;
    for (j, &w) in code.iter().enumerate().skip(head) {
        let i = Instruction(w);
        match i.opcode() {
            Some(O::Jmp) if j as i64 + 1 + i64::from(i.sj()) == head as i64 => {
                return (j - head + 1) as u64
            }
            Some(O::Forloop | O::Tforloop) if j as i64 + 1 - i64::from(i.bx()) == head as i64 => {
                return (j - head + 1) as u64
            }
            _ => {}
        }
    }
    8
}

/// The trace cache of one state (inside `JitData`).
pub struct TrState {
    pub(crate) cfg: TrConfig,
    slots: Vec<Slot>,
    free: Vec<u32>,
    active: Option<TrId>,
    /// Detached traces whose block and metadata are freed at the next safe point (TR18): flushed while an exit
    /// handler (or an entry-failure path) was running in their code.
    grave: Vec<Trace>,
    in_handler: u32,
    live_traces: u32,
    global_pen: i64,
    global_k: u8,
    /// P-G: time stamp of the first loop activity (0: none yet) and whether `TrLimits::warm` has elapsed.
    pub(crate) warm_t0: u64,
    pub(crate) warm_open: bool,
    /// BUNDLE-1: back-edges run since the start of the state (counted only while `TrLimits::wgate` is set and the gate is shut).
    pub(crate) warm_n: u64,
    /// BUNDLE-1: back-edges the head that fires next was armed for (0 = the threshold).
    pub(crate) warm_edges: u64,
    /// R1: the entry that `hot_loop` just granted to a rearm tick is not a stay for the entry monitor (consumed by `enter`).
    pub(crate) rearm_entering: bool,
    next_no: u32,
    pub(crate) stats: TrStats,
    starter: Option<StartFn>,
    side_starter: Option<StartSideFn>,
    dep_hook: Option<DepFreedFn>,
    /// F5: heads (descriptor key, head index) blacklisted by a flush; the nest rule runs on them in `request_flush`.
    nest_q: Vec<(usize, usize)>,
    /// BUNDLE-P: per trace, the readings of its previous hop-rate check ([`pre_exit_check`]). Per state (it was a thread-local, which let a second state
    /// on the same thread see the first one's readings).
    hops: BTreeMap<(u32, u32), HopSt>,
    /// BUNDLE-J (`centry`): code addresses of the prototypes that were first hot when a C function called them (read when the prototype is compiled).
    pub(crate) centry: std::collections::HashSet<usize>,
    /// BUNDLE-J: hot events of registered C-entered functions so far (key: code address; the entries start at the `centrywait`-th).
    pub(crate) centry_seen: std::collections::HashMap<usize, u32>,
}

/// BUNDLE-P: the state of one hop-rate check: `streak` the checks in a row that found the rate due.
#[derive(Clone, Copy)]
struct HopSt {
    streak: u32,
}

impl TrState {
    pub(crate) fn new(cfg: TrConfig) -> TrState {
        TrState {
            cfg: cfg.checked(),
            slots: Vec::new(),
            free: Vec::new(),
            active: None,
            grave: Vec::new(),
            in_handler: 0,
            live_traces: 0,
            global_pen: 0,
            global_k: 0,
            warm_t0: 0,
            warm_open: false,
            warm_n: 0,
            warm_edges: 0,
            rearm_entering: false,
            next_no: 1,
            stats: TrStats::default(),
            starter: None,
            side_starter: None,
            dep_hook: None,
            nest_q: Vec::new(),
            hops: BTreeMap::new(),
            centry: std::collections::HashSet::new(),
            centry_seen: std::collections::HashMap::new(),
        }
    }

    /// P-G: the first loop activity of the state happens now (starts the clock of `TrLimits::warm`).
    #[inline]
    pub(crate) fn warm_touch(&mut self, len: u64) {
        if !self.warm_open {
            self.warm_n += u64::from(rn_core::jit_hooks::tick_period().max(1) as u32) * len;
        }
        if self.cfg.limits.wgate != 0 {
            return;
        }
        if self.warm_t0 == 0 && !self.warm_open && self.cfg.limits.warm != 0 {
            self.warm_t0 = tsc().max(1);
        }
    }

    /// P-G: has `TrLimits::warm` ticks passed since the first loop activity? (Starts the clock when this is the first.)
    #[inline]
    pub(crate) fn warm_elapsed(&mut self, len: u64) -> bool {
        if self.warm_open || (self.cfg.limits.warm == 0 && self.cfg.limits.wgate == 0) {
            return true;
        }
        // one hot firing of a head stands for `hot_loop` iterations of a loop of `len` bytecodes
        let edges = if self.warm_edges != 0 {
            std::mem::take(&mut self.warm_edges)
        } else {
            self.cfg.hot_loop.max(1) as u64
        };
        self.warm_n += edges * len;
        if self.cfg.limits.wgate != 0 {
            if self.warm_n >= u64::from(self.cfg.limits.wgate) {
                self.warm_open = true;
            }
            return self.warm_open;
        }
        self.warm_touch(0);
        if tsc().wrapping_sub(self.warm_t0) >= u64::from(self.cfg.limits.warm) {
            self.warm_open = true;
        }
        self.warm_open
    }

    pub(crate) fn has_deferred(&self) -> bool {
        !self.grave.is_empty()
    }

    fn slot_mut(&mut self, id: TrId) -> Option<&mut Slot> {
        self.slots
            .get_mut(id.slot as usize)
            .filter(|s| s.gen == id.gen)
    }

    /// BUNDLE-P P1 / C45: for the family of the trace `id`: (side traces installed, a side recording of the family failed, a member unrolled a callee loop); (0, true, true)
    /// for anything but a trace. The family is clean when neither flag is set.
    pub(crate) fn family_clean(&self, id: TrId) -> (u32, bool, bool) {
        match self.slot(id) {
            Some(Slot {
                body: Body::Trace(t),
                ..
            }) => match self.slot(t.family) {
                Some(Slot {
                    body: Body::Trace(f),
                    ..
                }) => (
                    f.nside,
                    f.side_faults != 0 && !mutants::active("rearmclean_ignores_faults"),
                    f.la_any && !mutants::active("rearmclean_ignores_la"),
                ),
                _ => (0, true, true),
            },
            _ => (0, true, true),
        }
    }

    fn slot(&self, id: TrId) -> Option<&Slot> {
        self.slots.get(id.slot as usize).filter(|s| s.gen == id.gen)
    }

    fn alloc(&mut self, body: Body) -> TrId {
        if let Some(s) = self.free.pop() {
            let sl = &mut self.slots[s as usize];
            sl.body = body;
            TrId {
                slot: s,
                gen: sl.gen,
            }
        } else {
            self.slots.push(Slot { gen: 1, body });
            TrId {
                slot: self.slots.len() as u32 - 1,
                gen: 1,
            }
        }
    }

    fn release(&mut self, id: TrId) {
        let s = &mut self.slots[id.slot as usize];
        s.gen = s.gen.wrapping_add(1).max(1);
        s.body = Body::Free;
        self.free.push(id.slot);
    }
}

// ---- helpers over descriptors -----------------------------------------------------------------------------

/// A handle to the descriptor of a compiled prototype (an address; test and driver support). Valid while the
/// prototype lives; every function taking it checks the descriptor is still registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProtoRef(usize);

impl ProtoRef {
    /// The raw key (the `JitProto` address).
    pub fn key(self) -> usize {
        self.0
    }
}

fn tr_of<'a>(d: &JitData, key: usize) -> Option<&'a TrProto> {
    if !d.live.contains(&key) {
        return None;
    }
    // SAFETY: `key` is in `live`, i.e. a registered, not yet freed `JitProto` of this state; the table is a boxed
    // field that is never replaced, and the returned reference is used only while no code frees the descriptor
    // (all callers are synchronous and do not run Lua code or the collector while holding it).
    unsafe { (*(key as *const JitProto)).tr.as_deref() }
}

/// The compiled prototype running in the Lua frame below the current native frame (a test native calls this).
#[doc(hidden)]
pub fn caller_proto(st: &mut State) -> Option<ProtoRef> {
    let ci = st.ci().previous()?;
    ci_proto_ref(st, ci)
}

/// P-G: the block structure of `p` for the recorder (P-G, always on in the pipeline): the copy cached in the prototype's trace
/// table, filled by the first call; computed again, uncached, for a prototype without a table. The program, the resume
/// table and the region points are dropped from the cached copy (the count model does not read them).
pub fn landing_cached(p: &rn_core::func::Proto) -> Option<std::rc::Rc<Lowered>> {
    let jp = p.jit.get() as usize;
    if jp > rt::NOJIT {
        // SAFETY: as in `proto_ref`: a compiled prototype's `jit` is its live descriptor and `tr` is never replaced.
        if let Some(tp) = unsafe { (*(jp as *const JitProto)).tr.as_deref() } {
            if let Some(l) = tp.landing.borrow().as_ref() {
                return Some(l.clone());
            }
            let mut l = super::record::lower_for_landing(p)?;
            l.prog = Program::default();
            l.points = Vec::new();
            l.resume = Vec::new();
            let l = std::rc::Rc::new(l);
            *tp.landing.borrow_mut() = Some(l.clone());
            return Some(l);
        }
    }
    super::record::lower_for_landing(p).map(std::rc::Rc::new)
}

/// The [`ProtoRef`] the dependency edges (TR32) and [`with_tr`] key on, for a prototype of a state with traces on:
/// `None` when the prototype is not compiled, or has no trace table (it was compiled while traces were off, or it has
/// no candidate head). The handle is the descriptor's address: use it only while `p` is alive.
pub fn proto_ref(p: &rn_core::func::Proto) -> Option<ProtoRef> {
    let jp = p.jit.get() as usize;
    if jp <= rt::NOJIT {
        return None;
    }
    // SAFETY: a compiled prototype's `jit` is its live descriptor, which lives as long as the prototype (`p`); the
    // `tr` table is a boxed field that is never replaced.
    unsafe { (*(jp as *const JitProto)).tr.is_some() }.then_some(ProtoRef(jp))
}

fn ci_proto_ref(st: &mut State, ci: Ci) -> Option<ProtoRef> {
    let cl = rt::proto_of(st, ci)?;
    let jp = cl.proto().jit.get() as usize;
    (jp > rt::NOJIT).then_some(ProtoRef(jp))
}

/// Runs `f` on the trace table of `pr`.
#[doc(hidden)]
pub fn with_tr<R>(st: &mut State, pr: ProtoRef, f: impl FnOnce(&TrProto) -> R) -> Option<R> {
    // SAFETY: the per-state data is live.
    let d = unsafe { &*rt::data_ptr(st) };
    tr_of(d, pr.0).map(f)
}

fn d_of<'a>(st: &mut State) -> &'a mut JitData {
    // SAFETY: the per-state data is live as long as the state; callers do not keep two of these across a call that
    // takes the other (the usual discipline of `rt`: short-lived re-derived references).
    unsafe { &mut *rt::data_ptr(st) }
}

fn vlog(d: &JitData, s: impl FnOnce() -> String) {
    if d.tr.cfg.verbose >= 2 {
        eprintln!("[trace] {}", s());
    }
}

// ---- recording lifecycle ----------------------------------------------------------------------------------

/// Bytes charged transiently while a recording is active (3.5).
pub fn rec_bytes(l: &TrLimits) -> usize {
    l.maxrecord as usize * 16 + l.maxsnap as usize * 96
}

/// Why [`begin_recording`] did not start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Begin {
    /// The head is not `Warm`, or another recording is active, or a global penalty is pending.
    NotNow,
    /// The memory limit refused the buffers (transient penalty applied).
    Refused,
    /// No such head / descriptor.
    NoHead,
    /// Traces are off.
    Off,
}

/// Starts a recording of head `pc` of `pr` (what [`jit_tr_hot`] does before it calls the recorder): charges the
/// recording buffers, allocates the [`TrId`], marks the head `Recording`.
pub fn begin_recording(st: &mut State, pr: ProtoRef, pc: u32) -> Result<TrId, Begin> {
    let d = d_of(st);
    let Some(tp) = tr_of(d, pr.0) else {
        return Err(Begin::NoHead);
    };
    let Some(i) = tp.head_index(pc) else {
        return Err(Begin::NoHead);
    };
    if d.tr.cfg.mode == TrMode::Off || !d.enabled {
        return Err(Begin::Off);
    }
    let h = tp.head(i);
    if h.state.get() != HeadState::Warm || d.tr.active.is_some() {
        return Err(Begin::NotNow);
    }
    let bytes = rec_bytes(&d.tr.cfg.limits);
    if !jit_hooks::charge_code(st, bytes) {
        let d = d_of(st);
        d.tr.stats.refused += 1;
        let p = tp.on_transient(i);
        note_black(d, tp, i, p);
        if p == Penalty::Black {
            nest_rule(st, tp, i);
        }
        let d = d_of(st);
        vlog(d, || {
            format!("--- refused (memory limit) {}:{} pc {pc}", "proto", tp.line)
        });
        return Err(Begin::Refused);
    }
    let d = d_of(st);
    let no = d.tr.next_no;
    d.tr.next_no += 1;
    let id = d.tr.alloc(Body::Rec(Rec {
        root: pr.0,
        head: i,
        deps: Vec::new(),
        dep_freed: false,
        charged: bytes,
        exits: None,
        cells: None,
        cells_init: Vec::new(),
        side: None,
        no,
    }));
    d.tr.active = Some(id);
    d.tr.stats.started += 1;
    h.state.set(HeadState::Recording);
    h.trace.set(Some(id));
    vlog(d, || format!("{no} start proto:{} pc {pc}", tp.line));
    Ok(id)
}

/// F2a: starts a side recording at exit `exit` (a snapshot index) of the live trace `parent`: charges the recording
/// buffers, allocates the [`TrId`]. The head and its penalties are not involved (the family's root stays `Traced`).
pub fn begin_side_recording(st: &mut State, parent: TrId, exit: u32) -> Result<TrId, Begin> {
    begin_side_recording_ex(st, parent, exit, false)
}

/// [`begin_side_recording`]; `prelude`: the new trace will be the head's prelude (reached from `cell[H]`).
pub fn begin_side_recording_ex(
    st: &mut State,
    parent: TrId,
    exit: u32,
    prelude: bool,
) -> Result<TrId, Begin> {
    let d = d_of(st);
    if d.tr.cfg.mode == TrMode::Off || !d.enabled {
        return Err(Begin::Off);
    }
    let Some(Slot {
        body: Body::Trace(t),
        ..
    }) = d.tr.slot(parent)
    else {
        return Err(Begin::NoHead);
    };
    if d.tr.active.is_some()
        || (exit as usize) >= t.exit_st.len()
        || t.exit_st[exit as usize] != SideSt::Free
    {
        return Err(Begin::NotNow);
    }
    let (root, head) = (t.root, t.head);
    let bytes = rec_bytes(&d.tr.cfg.limits);
    if !jit_hooks::charge_code(st, bytes) {
        let d = d_of(st);
        d.tr.stats.refused += 1;
        return Err(Begin::Refused);
    }
    let d = d_of(st);
    let no = d.tr.next_no;
    d.tr.next_no += 1;
    let id = d.tr.alloc(Body::Rec(Rec {
        root,
        head,
        deps: Vec::new(),
        dep_freed: false,
        charged: bytes,
        exits: None,
        cells: None,
        cells_init: Vec::new(),
        side: Some(SideLink {
            parent,
            exit,
            prelude,
        }),
        no,
    }));
    d.tr.active = Some(id);
    d.tr.stats.side_started += 1;
    vlog(d, || {
        format!(
            "{no} side start from exit {exit} of trace slot {}",
            parent.slot
        )
    });
    Ok(id)
}

/// F2a: the exit `exit` of `parent` failed to produce a side trace (`fault`: a recording that aborted or did not
/// compile; otherwise the attempt just could not run now). The exit counter is re-armed so that it reports again after
/// `hot_exit` more exits; the [`MAX_SIDE_FAILS`]th failure blacklists the exit.
fn side_failed(
    d: &mut JitData,
    link: SideLink,
    fault: bool,
    reason: &'static str,
    unchangeable: bool,
) {
    vlog(d, || {
        format!(
            "side failed: parent {:?} exit {} fault {fault} {reason}",
            link.parent, link.exit
        )
    });
    if fault {
        d.tr.stats.side_failed += 1;
        // C45: the family root remembers it
        let fam = match d.tr.slot(link.parent) {
            Some(Slot {
                body: Body::Trace(p),
                ..
            }) => Some(p.family),
            _ => None,
        };
        if let Some(Slot {
            body: Body::Trace(f),
            ..
        }) = fam.and_then(|f| d.tr.slot_mut(f))
        {
            f.side_faults = f.side_faults.saturating_add(1);
        }
    }
    let sbump2 = d.tr.cfg.limits.sbump2;
    let Some(Slot {
        body: Body::Trace(t),
        ..
    }) = d.tr.slot_mut(link.parent)
    else {
        return;
    };
    let k = link.exit as usize;
    if k >= t.exit_st.len() {
        return;
    }
    let mut repeat = false;
    if fault {
        t.fails[k] = t.fails[k].saturating_add(1);
        repeat = unchangeable
            && t.last[k] == Some(reason)
            && !sbump2_pending(sbump2, reason, u32::from(t.fails[k]));
        t.last[k] = unchangeable.then_some(reason);
    }
    if reason == "SideUntraced"
        && t.fails[k] < MAX_SIDE_FAILS
        && !mutants::active("side_untraced_retry")
    {
        // F2b: wait for the head to get its trace (`install`); the counter stays past the divert value
        t.exit_st[k] = SideSt::Wait;
    } else if t.fails[k] >= MAX_SIDE_FAILS || repeat {
        // the counter stays where it is (past the divert value `hot_exit - 1`): the inline stub reports this exit no more
        black_exit(&mut t.exit_st, &t.exits, k);
    } else {
        // the inline stub reports an exit when its counter passes `hot_exit - 1`: count `hot_exit` more
        t.exits[k].set(0);
    }
}

/// BUNDLE-D (`sbump2`): the extra recursion unroll of the next side attempt at an exit inside inlined frames (`framed`) whose last attempts (`fails` of them, the last
/// ended `last`) were cut by the recursion / depth limits: one more level per failed attempt, at most `sbump2` (0 = off). Counters only, per exit.
pub fn sbump2_extra(sbump2: u32, last: Option<&'static str>, fails: u32, framed: bool) -> u32 {
    if sbump2 == 0
        || !framed
        || mutants::active("sbump2_never")
        || !matches!(last, Some("SideShort" | "Recursion" | "Depth"))
    {
        return 0;
    }
    if mutants::active("sbump2_unbounded") {
        fails
    } else {
        fails.min(sbump2)
    }
}

/// BUNDLE-D (`sbump2`): does a failure with `reason` (the `fails`th at its exit) leave a retry with more unroll pending? Then it is not a "repeat" (the second identical
/// unchangeable abort that blacklists the exit): the escalation gets its attempts, bounded by `sbump2` failed attempts and by `MAX_SIDE_FAILS`.
pub fn sbump2_pending(sbump2: u32, reason: &str, fails: u32) -> bool {
    sbump2 != 0
        && matches!(reason, "SideShort" | "Recursion" | "Depth")
        && fails <= sbump2
        && !mutants::active("sbump2_never")
}

/// H3-A: bit of an exit counter that marks a blacklisted exit: the frame-materialising stub's periodic report test masks it in
/// (`asm::Asm::mat_stub`), so a `Black` exit is never diverted to the handler again.
pub const EXIT_BLACK_BIT: u32 = 1 << 30;

/// H3-A: an exit counter without the [`EXIT_BLACK_BIT`] flag.
#[inline]
pub fn exit_count(c: u32) -> u32 {
    c & !EXIT_BLACK_BIT
}

/// H3-A: blacklists exit `k` of `t` (no side trace will ever be attached) and silences its periodic handler report.
fn black_exit(exit_st: &mut [SideSt], exits: &[Cell<u32>], k: usize) {
    exit_st[k] = SideSt::Black;
    if !mutants::active("black_exit_report") {
        exits[k].set(exits[k].get() | EXIT_BLACK_BIT);
    }
}

/// H3-A: is exit `k` of trace `id` blacklisted? (statistics of the exit handler)
pub fn exit_is_black(st: &mut State, id: TrId, k: usize) -> bool {
    matches!(
        d_of(st).tr.slot(id),
        Some(Slot { body: Body::Trace(t), .. }) if t.exit_st.get(k) == Some(&SideSt::Black)
    )
}

fn note_black(d: &mut JitData, tp: &TrProto, i: usize, p: Penalty) {
    if p == Penalty::Black {
        d.tr.stats.blacklisted += 1;
        if tp.is_cent() && tp.head(i).pc == 0 && !mutants::active("centry_black_keeps_entering") {
            // BUNDLE-J (`centry`): a C-entered function root that will never have a trace: calls from C stay in the interpreter (each entry costs more than it saves)
            tp.cent_off.set(true);
        }
        vlog(d, || {
            format!(
                "--- head pc {} of proto:{} blacklisted",
                tp.head(i).pc,
                tp.line
            )
        });
    }
}

/// Blacklists head `i` of `pr` the way a failing recording does: a trace of the head is flushed first, the head goes
/// `Black`, and (with `rule`) the nest rule of TR19b(a) runs (tests; the pipeline reaches the same code through its penalty
/// decisions). A head that is being recorded is left alone.
#[doc(hidden)]
pub fn blacklist_head(st: &mut State, pr: ProtoRef, i: usize, rule: bool) {
    let d = d_of(st);
    let Some(tp) = tr_of(d, pr.0) else {
        return;
    };
    match tp.head(i).state.get() {
        HeadState::Recording => return,
        HeadState::Traced => {
            if let Some(id) = tp.head(i).trace.get() {
                let r = flush_or_defer(d, id, FlushWhy::Nest);
                if r > 0 {
                    jit_hooks::uncharge_code(st, r);
                }
            }
        }
        _ => tp.blacklist(i),
    }
    d_of(st).tr.stats.blacklisted += 1;
    if rule {
        nest_rule(st, tp, i);
    }
}

/// F5: head `i` of `tp` went `Black`: applies the nest rule of TR19b(a) (flush the nest's traces, every head `Black`,
/// the nest goes to J4). Needs J4 (`RUSTYNAIL_JIT_T15`) and the rule enabled (`-nestrule` turns it off).
fn nest_rule(st: &mut State, tp: &TrProto, i: usize) {
    let d = d_of(st);
    if d.t15.mode == crate::t15::runtime::T15Mode::Off
        || !d.tr.cfg.opts.nestrule
        || mutants::active("nest_rule_skip")
    {
        return;
    }
    let ids = tp.nest_rule_traces(i);
    if ids.is_empty() {
        return;
    }
    // X5 restriction (docs/JIT_TRACE_REF.md "F5 decisions"): an inner trace that inlines calls is kept (closures: the
    // inner loop's trace is worth more than a J4 region that is usually not generated for a loop with calls)
    if !mutants::active("nest_rule_inlined")
        && ids.iter().any(|id| {
            matches!(d.tr.slot(*id), Some(Slot { body: Body::Trace(t), .. }) if !t.deps.is_empty())
        })
    {
        d.tr.stats.nest_rule_kept += 1;
        return;
    }
    d.tr.stats.nest_rule += 1;
    let mut refund = 0;
    for id in ids {
        refund += flush_or_defer(d, id, FlushWhy::Nest);
    }
    tp.nest_to_black(i);
    vlog(d, || {
        format!(
            "--- nest rule: outer head pc {} of proto:{} is black, the nest goes to J4",
            tp.head(i).pc,
            tp.line
        )
    });
    if refund > 0 {
        jit_hooks::uncharge_code(st, refund);
    }
}

/// Is the recording `id` still valid, i.e. neither stale nor flagged by a freed dependency (TR32)?
pub fn rec_aborted(st: &mut State, id: TrId) -> bool {
    match d_of(st).tr.slot(id) {
        Some(Slot {
            body: Body::Rec(r), ..
        }) => r.dep_freed,
        _ => true,
    }
}

fn rec_aborted_flag(d: &JitData, id: TrId) -> bool {
    matches!(d.tr.slot(id), Some(Slot { body: Body::Rec(r), .. }) if r.dep_freed)
}

/// Registers the DepFreed hook of this state (the recorder's flag setter).
pub fn set_dep_freed_hook(st: &mut State, f: Option<DepFreedFn>) {
    d_of(st).tr.dep_hook = f;
}

/// Registers the recorder entry for this state (overrides the process-wide one; tests).
pub fn set_starter(st: &mut State, f: Option<StartFn>) {
    d_of(st).tr.starter = f;
}

/// Allocates the exit counters of recording `id` (`n` words, address-stable) so that the assembler can embed their
/// addresses before the code exists; they move into the trace at [`install`].
pub fn alloc_exit_counters(st: &mut State, id: TrId, n: usize) -> Option<*const Cell<u32>> {
    match d_of(st).tr.slot_mut(id) {
        Some(Slot {
            body: Body::Rec(r), ..
        }) => {
            if let Some(b) = &r.exits {
                // a second call must not drop the first box (its address may be embedded in code already)
                return (b.len() == n).then_some(b.as_ptr());
            }
            let b: Box<[Cell<u32>]> = (0..n).map(|_| Cell::new(0)).collect();
            let p = b.as_ptr();
            r.exits = Some(b);
            Some(p)
        }
        _ => None,
    }
}

/// F2a: allocates the exit cells of recording `id` (`n` words, address-stable), initialised to the tier-1 landing
/// addresses `init[k]` (TR8: the stubs of the trace jump through them). They move into the trace at [`install`].
pub fn alloc_exit_cells(st: &mut State, id: TrId, init: &[usize]) -> Option<*const Cell<usize>> {
    match d_of(st).tr.slot_mut(id) {
        Some(Slot {
            body: Body::Rec(r), ..
        }) => {
            if let Some(b) = &r.cells {
                return (b.len() == init.len()).then_some(b.as_ptr());
            }
            let b: Box<[Cell<usize>]> = init.iter().map(|&a| Cell::new(a)).collect();
            let p = b.as_ptr();
            r.cells = Some(b);
            r.cells_init = init.to_vec();
            Some(p)
        }
        _ => None,
    }
}

/// TR32: the recorder inlined a call to the prototype `callee`: register the edge against the recording `id` *now*
/// (not at install). A stale `id` is ignored. Returns whether an edge exists afterwards.
pub fn register_dep(st: &mut State, id: TrId, callee: ProtoRef) -> bool {
    if mutants::active("dep_edge_skip") {
        return false;
    }
    let d = d_of(st);
    let Some(tp) = tr_of(d, callee.0) else {
        return false;
    };
    let Some(sl) = d.tr.slot_mut(id) else {
        return false;
    };
    match &mut sl.body {
        Body::Rec(r) => {
            if !r.deps.contains(&callee.0) {
                r.deps.push(callee.0);
            }
        }
        Body::Trace(t) => {
            if !t.deps.contains(&callee.0) {
                t.deps.push(callee.0);
            }
        }
        Body::Free => return false,
    }
    let mut v = tp.deps.borrow_mut();
    if !v.contains(&id) {
        v.push(id);
    }
    true
}

fn end_rec_common(d: &mut JitData, id: TrId) -> Option<Rec> {
    if d.tr.active == Some(id) {
        d.tr.active = None;
    }
    let sl = d.tr.slot_mut(id)?;
    let Body::Rec(r) = std::mem::replace(&mut sl.body, Body::Free) else {
        return None;
    };
    // drop the edges of the dying recording
    for dep in &r.deps {
        if let Some(tp) = tr_of(d, *dep) {
            tp.deps.borrow_mut().retain(|x| *x != id);
        }
    }
    d.tr.release(id);
    Some(r)
}

/// The recording `id` was aborted by the recorder: frees the id, refunds the buffers and applies the penalty rule
/// (`unchangeable` causes twice in a row blacklist). A stale id is ignored.
pub fn abort_recording(st: &mut State, id: TrId, reason: &'static str, unchangeable: bool) {
    finish_abort(st, id, reason, AbortKind::Abort(unchangeable));
}

/// N3a: a side recording that had executed a `RETF` failed (any reason): the exit is not at fault, it is retried once without `RETF` (so the trace that ends
/// before the `RETURN` is not lost to the longer recording).
pub fn abort_retf(st: &mut State, id: TrId, reason: &'static str) {
    let d = d_of(st);
    let link = match d.tr.slot(id) {
        Some(Slot {
            body: Body::Rec(r), ..
        }) => r.side,
        _ => None,
    };
    if let Some(l) = link {
        if let Some(Slot {
            body: Body::Trace(t),
            ..
        }) = d.tr.slot_mut(l.parent)
        {
            if let Some(f) = t.noretf.get_mut(l.exit as usize) {
                *f = true;
            }
        }
    }
    finish_abort(st, id, reason, AbortKind::Drop);
}

/// Like [`abort_recording`] for an `Unstable` re-record (no penalty, `rerec += 1`, the next back-edge records again).
pub fn abort_unstable(st: &mut State, id: TrId) {
    finish_abort(st, id, "Unstable", AbortKind::Unstable);
}

/// The compile of the recording was refused for a transient reason (code cap, memory limit).
pub fn abort_transient(st: &mut State, id: TrId, reason: &'static str) {
    finish_abort(st, id, reason, AbortKind::Transient);
}

/// How a dying recording treats its head.
#[derive(Clone, Copy)]
enum AbortKind {
    /// Recorder abort (`unchangeable` cause).
    Abort(bool),
    /// `Unstable` re-record.
    Unstable,
    /// Transient refusal.
    Transient,
    /// Q3: identical to the generation flushed last.
    Identical,
    /// Not the head's fault (DepFreed, abandoned, traces off): no penalty.
    Drop,
}

fn finish_abort(st: &mut State, id: TrId, reason: &'static str, kind: AbortKind) {
    let d = d_of(st);
    let Some(r) = end_rec_common(d, id) else {
        return;
    };
    jit_hooks::uncharge_code(st, r.charged);
    let d = d_of(st);
    if let Some(link) = r.side {
        *d.tr.stats.side_aborted.entry(reason).or_insert(0) += 1;
        // a side recording never touches its head: only the exit it hangs off remembers the failure
        let (fault, unchangeable) = match kind {
            AbortKind::Drop => (false, false),
            AbortKind::Abort(u) => (true, u),
            _ => (true, false),
        };
        side_failed(d, link, fault, reason, unchangeable);
        vlog(d, || {
            format!(
                "--- side abort exit {} of trace slot {} {reason}",
                link.exit, link.parent.slot
            )
        });
        return;
    }
    *d.tr.stats.aborted.entry(reason).or_insert(0) += 1;
    // the root was freed meanwhile (`on_proto_free` zeroed it: a new descriptor may live at the same address)
    let Some(tp) = tr_of(d, r.root).filter(|_| r.root != 0) else {
        return;
    };
    let p = match kind {
        AbortKind::Unstable => {
            d.tr.stats.rerec += 1;
            tp.on_rerecord(r.head)
        }
        AbortKind::Abort(u) => {
            // J2-2: a loop recording that hit the recursion / depth limit gets one deeper re-record
            if d.tr.cfg.limits.deep != 0
                && matches!(reason, "Recursion" | "Depth")
                && !tp.is_rec_head(tp.head(r.head).pc)
                && !mutants::active("deep_never")
                && tp.on_deep(r.head)
            {
                d.tr.stats.deep_sites += 1;
                vlog(d, || {
                    format!(
                        "--- deep site proto:{} pc {} after {reason}",
                        tp.line,
                        tp.head(r.head).pc
                    )
                });
                Penalty::Wait(1)
            } else {
                let p = tp.on_abort(r.head, reason, u);
                if second_retry_due(d.tr.cfg.limits.second, p == Penalty::Black, u, reason) {
                    if let Some(w) = tp.second_chance(r.head) {
                        d.tr.stats.retried += 1;
                        w
                    } else {
                        p
                    }
                } else if p == Penalty::Black
                    && reason == "FnShort"
                    && d.tr.cfg.limits.fnpromo != 0
                    && tp.is_fn_head(tp.head(r.head).pc)
                    && !tp.recursive.get()
                    && d.tr.stats.fn_promoted < u64::from(d.tr.cfg.limits.fnpromo)
                {
                    if let Some(w) = tp.promote_fn(r.head, d.tr.cfg.limits.fnpromow) {
                        d.tr.stats.fn_promoted += 1;
                        vlog(d, || {
                            format!("--- fnpromo: function head pc {} of proto:{} (code {}) becomes a generic function root", tp.head(r.head).pc, tp.line, tp.pcs.len())
                        });
                        w
                    } else {
                        p
                    }
                } else if p == Penalty::Black && reason == "InnerLoop" {
                    if let Some(w) = tp.inner_chance(r.head, d.tr.cfg.limits.innerretry) {
                        d.tr.stats.inner_retried += 1;
                        vlog(d, || {
                            format!(
                                "--- innerretry: head pc {} of proto:{} gets round {}",
                                tp.head(r.head).pc,
                                tp.line,
                                tp.heads[r.head].inner_n.get()
                            )
                        });
                        w
                    } else {
                        p
                    }
                } else {
                    p
                }
            }
        }
        AbortKind::Transient => {
            d.tr.stats.refused += 1;
            tp.on_transient(r.head)
        }
        AbortKind::Identical => tp.on_identical(r.head),
        AbortKind::Drop => {
            tp.on_dropped(r.head);
            Penalty::Wait(tp.hot)
        }
    };
    note_black(d, tp, r.head, p);
    if p == Penalty::Black {
        nest_rule(st, tp, r.head);
    }
    let d = d_of(st);
    vlog(d, || {
        format!(
            "--- abort {}:{} pc {} {reason} ({p:?})",
            "proto",
            tp.line,
            tp.head(r.head).pc
        )
    });
}

// ---- install ----------------------------------------------------------------------------------------------

/// What the pipeline driver hands over to be installed (the machine code is position independent).
pub struct TraceInstall {
    /// The code block's contents.
    pub code: Vec<u8>,
    /// Offset of the trace entry in `code`.
    pub entry_off: usize,
    /// P-M3: offset of the inner entry (native entries), 0 if the trace has none.
    pub inner_off: usize,
    /// P-M3: the trace ends with a native return.
    pub rec_ret: bool,
    /// `RUSTYNAIL_JIT_DUMP` text (IR and so on); written to `trace-<n>.txt` when a dump directory is set.
    pub text: Option<String>,
    /// Q3: [`shape_hash`] of the normalised IR / register allocation text (identifies a repeated compile).
    pub hash: u64,
    /// BUNDLE-J: number of IR instructions of the trace (the `centry` weight rule).
    pub weight: u32,
}

/// Why [`install`] did not install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallError {
    /// `id` is stale or not a recording.
    Stale,
    /// A dependency prototype was freed during the recording (TR32); the recording was aborted.
    DepFreed,
    /// Transient refusal (memory limit / code cap / trace limit); the recording was aborted with a transient penalty.
    Refused,
    /// Permanent failure (the code could not be placed); the recording was aborted.
    Failed,
    /// Traces are off (mode `off` or the compiler disabled); the recording was aborted without penalty.
    Off,
}

/// FNV-1a of the normalised shape text of a compile (IR + register allocation); never 0.
pub fn shape_hash(shape: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in shape.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h | 1
}

/// FNV-1a over 64-bit words (the structural shape hash below).
struct Fnv(u64);

impl Fnv {
    fn word(&mut self, w: u64) {
        self.0 = (self.0 ^ w).wrapping_mul(0x0000_0100_0000_01b3);
        self.0 ^= self.0 >> 29;
    }
}

impl std::hash::Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.word(u64::from(b));
        }
    }
    fn write_u8(&mut self, i: u8) {
        self.word(u64::from(i));
    }
    fn write_u16(&mut self, i: u16) {
        self.word(u64::from(i));
    }
    fn write_u32(&mut self, i: u32) {
        self.word(u64::from(i));
    }
    fn write_u64(&mut self, i: u64) {
        self.word(i);
    }
    fn write_usize(&mut self, i: usize) {
        self.word(i as u64);
    }
    fn write_isize(&mut self, i: isize) {
        self.word(i as u64);
    }
}

/// P-G: the shape hash of a compile computed from the IR and the register allocation themselves (the same data the
/// `print::dump` / `ra::dump` text of Q3 shows: instructions, constants, snapshots and their entries, locations, moves),
/// without formatting the text (which cost about as much as the rest of the pipeline on a short trace). Equal shapes hash
/// equal; never 0.
pub fn shape_hash_ir(ir: &super::ir::TraceIr, al: &super::ra::Allocation) -> u64 {
    use std::hash::Hash;
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    ir.ins.len().hash(&mut h);
    for (i, ins) in ir.ins.iter().enumerate() {
        ins.op.hash(&mut h);
        ins.t.hash(&mut h);
        h.word(
            u64::from(ins.flags)
                | u64::from(ins.a) << 8
                | u64::from(ins.b) << 24
                | u64::from(ins.aux) << 40,
        );
        h.word(u64::from(ins.aux >> 24));
        let ia = &al.ins[i];
        ia.dst.hash(&mut h);
        ia.ops.hash(&mut h);
        ia.stores.hash(&mut h);
        for r in ia.pre.iter().chain(&ia.post) {
            r.reg.hash(&mut h);
            h.word(u64::from(r.slot));
        }
        h.word(ia.pre.len() as u64);
        h.word(u64::from(al.fused[i]));
    }
    ir.k.hash(&mut h);
    ir.loop_at.hash(&mut h);
    for (n, sn) in ir.snaps.iter().enumerate() {
        h.word(u64::from(sn.pc) | u64::from(sn.frame) << 32);
        h.word(sn.refund as u32 as u64);
        for e in ir.snap_entries(n as u32) {
            h.word(u64::from(e.slot) | u64::from(e.r) << 16 | u64::from(e.flags) << 32);
        }
        h.word(u64::MAX);
    }
    al.backedge_after.hash(&mut h);
    for l in &al.snap_locs {
        l.hash(&mut h);
    }
    for m in al.entry_seq.iter().chain(&al.backedge_seq) {
        m.dst.hash(&mut h);
        m.src.hash(&mut h);
    }
    al.entry_seq.len().hash(&mut h);
    use std::hash::Hasher;
    h.finish() | 1
}

fn trace_meta_bytes(exits: usize, deps: usize) -> usize {
    std::mem::size_of::<Trace>() + exits * 4 + deps * std::mem::size_of::<usize>()
}

/// Installs the compiled trace of recording `id`: places the code (charged, refunded on flush), turns the slot into a
/// trace, marks the head `Traced` and points `cell[H]` at the entry (or the `+tracemove` stub).
pub fn install(st: &mut State, id: TrId, ti: TraceInstall) -> Result<TrId, InstallError> {
    let d = d_of(st);
    let (root, head, dep_freed, ndeps, no, nexits, side) = match d.tr.slot(id) {
        Some(Slot {
            body: Body::Rec(r), ..
        }) => (
            r.root,
            r.head,
            r.dep_freed,
            r.deps.len(),
            r.no,
            r.exits.as_ref().map_or(0, |e| e.len()),
            r.side,
        ),
        _ => return Err(InstallError::Stale),
    };
    if d.tr.cfg.mode == TrMode::Off || !d.enabled {
        finish_abort(st, id, "Off", AbortKind::Drop);
        return Err(InstallError::Off);
    }
    if dep_freed {
        finish_abort(st, id, "DepFreed", AbortKind::Drop);
        return Err(InstallError::DepFreed);
    }
    // one more trace than allowed: flush all (global penalty), refuse this one
    if d.tr.live_traces >= d.tr.cfg.limits.maxtrace {
        flush_all(st, FlushWhy::Cap);
        abort_transient(st, id, "TraceLimit");
        return Err(InstallError::Refused);
    }
    // F2a: the parent trace must still exist with this exit free (it was flushed, or the exit got another trace)
    let family = match side {
        Some(sl) => match d.tr.slot(sl.parent) {
            Some(Slot {
                body: Body::Trace(p),
                ..
            }) if p.exit_st.get(sl.exit as usize) == Some(&SideSt::Free)
                && (!sl.prelude
                    || tr_of(d, p.root).is_some_and(|tp| {
                        let h = tp.head(p.head);
                        h.state.get() == HeadState::Traced
                            && h.trace.get() == Some(p.family)
                            && h.pre.get() == 0
                    })) =>
            {
                p.family
            }
            _ => {
                finish_abort(st, id, "ParentGone", AbortKind::Drop);
                return Err(InstallError::Stale);
            }
        },
        None => id,
    };
    // BUNDLE-J (`centry`): a C-entered function root that is too light to repay the entry from C: the prototype stays in the interpreter for calls from C
    // (the trace is installed all the same: flat calls from compiled callers can use it)
    if side.is_none() && d.tr.cfg.limits.centry != 0 {
        if let Some(tp) = tr_of(d, root) {
            if tp.is_cent() {
                vlog(d, || {
                    format!(
                        "centry: root of head {head} has {} IR instructions",
                        ti.weight
                    )
                });
            }
            if tp.is_cent()
                && tp.head(head).pc == 0
                && ti.weight < d.tr.cfg.limits.centrymin
                && !mutants::active("centry_weight_ignored")
            {
                tp.cent_off.set(true);
                d.tr.stats.c_light += 1;
            }
        }
    }
    // Q3: a compile byte-identical to the generation flushed last for this head would fail the same way
    let hash = ti.hash;
    if side.is_none() && !mutants::active("identical_reinstall") {
        let same =
            hash != 0 && tr_of(d, root).is_some_and(|tp| tp.head(head).flushed_hash.get() == hash);
        // C36 (`second` bit 1): the first identical re-record of a head is installed: the flush that preceded it (a hot exit, a burst of entry failures) was no evidence
        // against the code, and blacklisting would leave the loop in tier 1 for good; a second identical compile still blacklists
        let keep = same
            && tr_of(d, root).is_some_and(|tp| {
                let h = tp.head(head);
                let due = ident_keep_due(d.tr.cfg.limits.second, h.ident_kept.get());
                if due {
                    h.ident_kept.set(true);
                }
                due
            });
        if keep {
            d.tr.stats.ident_kept += 1;
            vlog(d, || {
                format!("second: identical re-record of head {head} installed once")
            });
        } else if same {
            finish_abort(st, id, "Identical", AbortKind::Identical);
            return Err(InstallError::Refused);
        }
    }
    let meta = trace_meta_bytes(nexits, ndeps);
    if !jit_hooks::charge_code(st, meta) {
        abort_transient(st, id, "MemLimit");
        return Err(InstallError::Refused);
    }
    let dp = rt::data_ptr(st);
    let placed = match rt::place_code(st, dp, &ti.code, usize::from(super::asm::LINE)) {
        Ok(p) => p,
        Err(e) => {
            jit_hooks::uncharge_code(st, meta);
            let d = d_of(st);
            let capped = d.arena.mapped() + ti.code.len() > d.arena.cap();
            if e == Fail::Retry {
                if capped && d.tr.live_traces > 0 {
                    flush_all(st, FlushWhy::Cap);
                }
                abort_transient(st, id, "CodeCap");
                return Err(InstallError::Refused);
            }
            abort_recording(st, id, "PlaceFailed", true);
            return Err(InstallError::Failed);
        }
    };
    let d = d_of(st);
    let entry = placed.block.addr() + ti.entry_off;
    let block_addr = placed.block.addr();
    let sl = d.tr.slot_mut(id).expect("checked above");
    let Body::Rec(r) = std::mem::replace(&mut sl.body, Body::Free) else {
        unreachable!("checked above")
    };
    let rec_charge = r.charged;
    let exits = r.exits.unwrap_or_default();
    let cells = r.cells.unwrap_or_default();
    let cells_init = r.cells_init;
    let nex = exits.len();
    sl.body = Body::Trace(Trace {
        root: r.root,
        head: r.head,
        deps: r.deps,
        block: Some(placed.block),
        own: placed.own,
        meta,
        exits,
        hash,
        xmeta: None,
        no,
        entry,
        cells,
        cells_init,
        side,
        family,
        kids: Vec::new(),
        exit_st: vec![SideSt::Free; nex],
        fails: vec![0; nex],
        noretf: vec![false; nex],
        last: vec![None; nex],
        mark: vec![None; nex],
        nside: 0,
        no_start_side: ti.rec_ret,
        gk: Vec::new(),
        la: Vec::new(),
        la_sides: 0,
        side_faults: 0,
        la_any: false,
        defer: vec![0; nex],
        dbl: Vec::new(),
        entry_callee: false,
        hoisted: false,
        chain_done: vec![0; nex],
        chain_n: 0,
        chain_off: vec![0; nex],
    });
    d.tr.active = None;
    d.tr.live_traces += 1;
    if let Some(sl) = side {
        // TR8: the parent's exit continues in the new trace from now on; nothing of the parent's code changes
        d.tr.stats.side_installed += 1;
        if sl.prelude {
            d.tr.stats.side_preludes += 1;
        }
        if let Some(Slot {
            body: Body::Trace(p),
            ..
        }) = d.tr.slot_mut(sl.parent)
        {
            p.kids.push(id);
            p.exit_st[sl.exit as usize] = SideSt::Has(id);
            if !sl.prelude {
                if let Some(c) = p.cells.get(sl.exit as usize) {
                    c.set(entry);
                }
            }
        }
        if sl.prelude {
            // the head's dispatch goes through the prelude from now on (checked above)
            if let Some(tp) = tr_of(d, root) {
                let h = tp.head(head);
                h.pre.set(entry);
                h.retarget();
            }
        }
        if let Some(Slot {
            body: Body::Trace(f),
            ..
        }) = d.tr.slot_mut(family)
        {
            f.nside += 1;
        }
        vlog(d, || {
            format!(
                "{no} side{} exit {} of trace slot {} entry {entry:#x} code {}B",
                if sl.prelude { " (prelude)" } else { "" },
                sl.exit,
                sl.parent.slot,
                ti.code.len()
            )
        });
    } else {
        d.tr.stats.installed += 1;
    }
    if side.is_none() {
        // F2b: exits that waited for a root trace in this prototype try again (their counters start over)
        let retf_wake = d.tr.cfg.limits.retf & 1 != 0;
        for sl in d.tr.slots.iter_mut() {
            if let Body::Trace(t) = &mut sl.body {
                // (N3a: with `retf` a side trace that popped into a caller waits for a root trace in THAT prototype: any install wakes the waiting exits)
                if t.root == root || retf_wake {
                    for k in 0..t.exit_st.len() {
                        if t.exit_st[k] == SideSt::Wait {
                            t.exit_st[k] = SideSt::Free;
                            t.exits[k].set(0);
                        }
                    }
                }
            }
        }
    }
    if side.is_some() {
        // no head transition: the family's root keeps the head
    } else if let Some(tp) = tr_of(d, root) {
        let h = tp.head(head);
        h.entry.set(entry);
        h.rec_inner.set(if ti.inner_off != 0 {
            block_addr + ti.inner_off
        } else {
            0
        });
        h.trace.set(Some(id));
        tp.set_state(head, HeadState::Traced);
        if d.live.contains(&root) {
            // N3b: the entries the monitor judged ran tier 1 code that left at once; a trace now runs the loop
            // SAFETY: `root` is in `live` (a registered, not yet freed `JitProto` of this state).
            unsafe { (*(root as *const JitProto)).reset_entry_backoff() };
        }
        vlog(d, || {
            format!(
                "{no} loop proto:{} pc {} entry {entry:#x} code {}B",
                tp.line,
                h.pc,
                ti.code.len()
            )
        });
    }
    if let (Some(dir), Some(text)) = (d.tr.cfg.dump.as_deref(), ti.text.as_deref()) {
        let _ = std::fs::write(
            format!("{dir}/trace-{no}.txt"),
            print::trace_file(no, block_addr, entry, text),
        );
        // the machine code (`objdump -D -b binary -mi386:x86-64 trace-N.bin`; the block starts at `block_addr`)
        let _ = std::fs::write(format!("{dir}/trace-{no}.bin"), &ti.code);
    }
    jit_hooks::uncharge_code(st, rec_charge);
    Ok(id)
}

// ---- flush ------------------------------------------------------------------------------------------------

/// Detaches the trace `id` (if it is one and not stale): the slot is released (the id is stale from now on, which
/// dedupes repeated requests), the head returns to the state `why` asks for (`cell[H]` is re-targeted), the
/// dependency edges are dropped. Returns the trace with its block and metadata *still allocated*; the caller frees
/// them now ([`retire`]) or, when an exit handler runs in that code, at the next safe point (TR18).
fn flush_now(d: &mut JitData, id: TrId, why: FlushWhy) -> Option<Trace> {
    let sl = d.tr.slot_mut(id)?;
    if !matches!(sl.body, Body::Trace(_)) {
        return None; // a recording is aborted by its owner (`abort_recording`), never flushed
    }
    let Body::Trace(t) = std::mem::replace(&mut sl.body, Body::Free) else {
        unreachable!("checked above")
    };
    d.tr.release(id);
    d.tr.live_traces -= 1;
    if t.side.is_some() {
        d.tr.stats.side_flushes += 1;
    } else {
        d.tr.stats.flushes += 1;
    }
    d.tr.stats.exits += trace_exits(&t);
    exit_dump(&t, "flushed");
    let hot = d.tr.cfg.hot_loop;
    let mut blacklisted = false;
    let mut entries = 0;
    if let Some(tp) = tr_of(d, t.root) {
        let h = tp.head(t.head);
        entries = h.entry_ok.get();
        if h.trace.get() == Some(id) {
            if matches!(why, FlushWhy::Rerec | FlushWhy::Ineffective) {
                h.flushed_hash.set(t.hash);
            }
            let p = match why {
                FlushWhy::Rerec | FlushWhy::Ineffective => {
                    d.tr.stats.rerec += 1;
                    if why == FlushWhy::Ineffective {
                        d.tr.stats.ineffective += 1;
                    }
                    if why == FlushWhy::Ineffective {
                        tp.on_ineffective(t.head)
                    } else {
                        tp.on_rerecord(t.head)
                    }
                }
                FlushWhy::Dep
                | FlushWhy::Cap
                | FlushWhy::Unsink
                | FlushWhy::Dom
                | FlushWhy::Chain => {
                    tp.to_warm(t.head, i64::from(hot));
                    Penalty::Wait(hot)
                }
                FlushWhy::Off => {
                    tp.to_warm(t.head, i64::from(i32::MAX / 2));
                    Penalty::Wait(i32::MAX / 2)
                }
                FlushWhy::Nest => {
                    h.trace.set(None);
                    h.entry.set(0);
                    h.rec_inner.set(0);
                    h.rec_inner.set(0);
                    h.pre.set(0);
                    tp.blacklist(t.head);
                    Penalty::Black
                }
            };
            blacklisted = p == Penalty::Black;
        }
    }
    if blacklisted {
        d.tr.stats.blacklisted += 1;
        if why != FlushWhy::Nest {
            d.tr.nest_q.push((t.root, t.head));
        }
    }
    for dep in &t.deps {
        if let Some(tp) = tr_of(d, *dep) {
            tp.deps.borrow_mut().retain(|x| *x != id);
        }
    }
    // F2a (TR18): the stubs of a flushed trace may still run (the flush was requested inside an exit handler or an
    // entry-failure path of its own code): every frameless exit of it lands in tier 1 from now on, never in another
    // trace of the family, whose block is about to be freed too.
    if let Some(m) = &t.xmeta {
        for (c, sn) in t.cells.iter().zip(&m.snaps) {
            if sn.land_addr != 0 && !mutants::active("flush_keeps_cells") {
                c.set(sn.land_addr as usize);
            }
        }
    }
    if t.side.is_some_and(|sl| sl.prelude) {
        // F2a: the head's prelude goes away: its dispatch cell targets the root (or tier 1) again
        if let Some(tp) = tr_of(d, t.root) {
            let h = tp.head(t.head);
            if h.pre.get() == t.entry {
                h.pre.set(0);
                if h.state.get() == HeadState::Traced {
                    h.retarget();
                }
            }
        }
    }
    if let Some(sl) = t.side {
        // F2a: a side trace goes away alone (a dependency freed): its parent's exit lands in tier 1 again, at once
        // (TR8: the cell is the only link, nothing else refers to the block). When the parent is flushed first (the
        // family goes with its root) its slot is stale and there is nothing to restore.
        if let Some(Slot {
            body: Body::Trace(p),
            ..
        }) = d.tr.slot_mut(sl.parent)
        {
            let k = sl.exit as usize;
            p.kids.retain(|x| *x != id);
            if p.exit_st.get(k) == Some(&SideSt::Has(id)) {
                p.exit_st[k] = SideSt::Free;
            }
            if let (Some(&home), Some(c)) = (p.cells_init.get(k), p.cells.get(k)) {
                if !mutants::active("side_cell_stale") {
                    c.set(home);
                }
            }
            if let Some(c) = p.exits.get(k) {
                c.set(0);
            }
            // (P3: the counts before a re-base of the counter belong to the flushed side trace's chain)
            if let Some(o) = p.chain_off.get_mut(k) {
                *o = 0;
            }
        }
        if let Some(Slot {
            body: Body::Trace(f),
            ..
        }) = d.tr.slot_mut(t.family)
        {
            f.nside = f.nside.saturating_sub(1);
        }
    }
    vlog(d, || {
        format!(
            "{} flush {why:?} (entries {entries}, exits {}; by snapshot {:?})",
            t.no,
            trace_exits(&t),
            t.exits
                .iter()
                .enumerate()
                .filter(|(_, c)| c.get() > 0)
                .map(|(k, c)| (k, c.get()))
                .collect::<Vec<_>>()
        )
    });
    Some(t)
}

/// Detaches `id` and the side traces hanging off it (F2a: a family goes with its root; the descendants have no head to
/// transition). The traces are returned parent first, blocks still allocated.
fn flush_tree(d: &mut JitData, id: TrId, why: FlushWhy) -> Vec<Trace> {
    let mut out = Vec::new();
    let Some(mut t) = flush_now(d, id, why) else {
        return out;
    };
    let kids = std::mem::take(&mut t.kids);
    out.push(t);
    if !mutants::active("side_family_flush_skip") {
        for k in kids {
            out.extend(flush_tree(d, k, why));
        }
    }
    out
}

/// Frees the block and metadata of a detached trace; returns the bytes to refund.
fn retire(d: &mut JitData, t: Trace) -> usize {
    if let Some(b) = t.block {
        // SAFETY: TR18: no activation of the trace exists (TR1/TR2) except inside the exit handler, and a trace
        // detached there is retired at a safe point instead; the head cell no longer points into the block.
        let _ = unsafe { d.arena.free(b) };
    }
    t.own + t.meta
}

/// Detaches `id` and retires it now, or parks it in the graveyard while an exit handler runs. Returns the bytes
/// to refund now.
fn flush_or_defer(d: &mut JitData, id: TrId, why: FlushWhy) -> usize {
    let ts = flush_tree(d, id, why);
    if ts.is_empty() {
        return 0;
    }
    if d.tr.in_handler > 0 && !mutants::active("flush_in_handler") {
        d.tr.stats.deferred += ts.len() as u64;
        d.tr.grave.extend(ts);
        0
    } else {
        ts.into_iter().map(|t| retire(d, t)).sum()
    }
}

/// Flush request (TR18): the head transition happens now (so a hot exit or a failing entry is reported once and
/// the head re-records at the next back-edge); the block and metadata are freed now, or at the next safe point
/// when an exit handler is running (a trace block must not be freed under the handler that was called from it).
pub fn request_flush(st: &mut State, id: TrId, why: FlushWhy) {
    let r = flush_or_defer(d_of(st), id, why);
    if r > 0 {
        jit_hooks::uncharge_code(st, r);
    }
    drain_nest_q(st);
}

/// F5: runs the nest rule on the heads a flush blacklisted.
fn drain_nest_q(st: &mut State) {
    let q = std::mem::take(&mut d_of(st).tr.nest_q);
    for (key, head) in q {
        let d = d_of(st);
        if let Some(tp) = tr_of(d, key) {
            nest_rule(st, tp, head);
        }
    }
}

/// Flushes every trace of the state (`maxtrace` / code cap: followed by a global penalty of `2^k * 1024`
/// back-edges; mode off: no penalty).
pub fn flush_all(st: &mut State, why: FlushWhy) {
    if why == FlushWhy::Off {
        if let Some(aid) = d_of(st).tr.active {
            finish_abort(st, aid, "Off", AbortKind::Drop);
        }
    }
    let d = d_of(st);
    let ids: Vec<TrId> =
        d.tr.slots
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.body, Body::Trace(_)))
            .map(|(i, s)| TrId {
                slot: i as u32,
                gen: s.gen,
            })
            .collect();
    if why == FlushWhy::Cap {
        d.tr.global_k = (d.tr.global_k + 1).min(10);
        d.tr.global_pen = 1024i64 << d.tr.global_k;
    }
    for id in ids {
        request_flush(st, id, why);
    }
}

/// P-V: the string metatable was replaced. Traces resolve `s:method` through the metatable they saw (a constant of the trace),
/// so every trace of the state is flushed and a recording in progress is dropped. No penalty: the heads re-record at once.
pub fn type_mt_changed(st: &mut State) {
    if mutants::active("strmt_flush_skip") {
        return;
    }
    if let Some(aid) = d_of(st).tr.active {
        finish_abort(st, aid, "Off", AbortKind::Drop);
    }
    flush_all(st, FlushWhy::Dep);
}

/// `jit.flush()`: a recording in progress is dropped and every trace is flushed (no penalty: the heads re-record).
pub fn control_flush(st: &mut State) {
    if let Some(aid) = d_of(st).tr.active {
        finish_abort(st, aid, "Off", AbortKind::Drop);
    }
    flush_all(st, FlushWhy::Dep);
}

/// Marks entry into the Rust exit handler (called from trace code): flushes requested until [`handler_leave`] are
/// deferred.
pub fn handler_enter(st: &mut State) {
    d_of(st).tr.in_handler += 1;
}

/// Leaves the exit handler. Deferred flushes run at the next safe point ([`safe_point`]).
pub fn handler_leave(st: &mut State) {
    let d = d_of(st);
    d.tr.in_handler = d.tr.in_handler.saturating_sub(1);
}

/// Retires the traces parked in the graveyard if no handler is active (called from `jit_tr_hot`, `rt::enter`, the
/// end of a recording and state close). `free_code` is *not* a safe point: the collector may run inside a handler.
pub fn safe_point(st: &mut State) {
    let d = d_of(st);
    if d.tr.in_handler > 0 || d.tr.grave.is_empty() {
        return;
    }
    let q = std::mem::take(&mut d.tr.grave);
    let mut refund = 0;
    for t in q {
        refund += retire(d, t);
    }
    jit_hooks::uncharge_code(st, refund);
}

/// What the exit handler does after [`exit_hot_policy`] (F2a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotAction {
    /// Nothing: the exit keeps landing where its cell says.
    None,
    /// The family of the trace was flushed (deferred, TR18): the prototype's re-record rule.
    Flushed,
    /// A side recording was armed at the exit's pc: the handler leaves compiled code for the interpreter at `savedpc`
    /// after adding this many instructions (`rem` of the landing label: tier 1 gives the prepaid rest of the block back
    /// before it leaves) to the budget word.
    Interp(u32),
}

/// Who starts a side recording (the pipeline driver's `driver_start_side`): arm the recorder for the running frame
/// `ci` at `start.pc`; `Some(rem)`: armed (`rem` = instructions the landing label had prepaid), `None`: refused.
pub type StartSideFn = fn(&mut State, Ci, TrId, &SideStart) -> Option<u32>;

/// What a hot exit tells the side recorder.
#[derive(Clone, Debug)]
pub struct SideStart {
    /// The pc the parent's exit snapshot re-executes (in the root frame).
    pub pc: u32,
    /// The slots the exit stub restores, with their types.
    pub known: Vec<(u16, super::ir::Ty)>,
    /// The new trace is the head's prelude (see [`HeadData::pre`]).
    pub prelude: bool,
    /// S1: the inlined frames of the exit snapshot (outermost first): the side recording starts inside them with the
    /// frames virtual. Empty for an exit of the root frame.
    pub frames: Vec<SideFrame>,
    /// S1: what the parent's entry checks guarantee (stack extent, spare `CallInfo` nodes): a side trace with frames runs
    /// without entry checks and may not need more.
    pub max_extent: u16,
    /// See `max_extent`: spare `CallInfo` nodes.
    pub max_ci: u8,
    /// BUNDLE-C: extra recursion unrolls for this recording (see [`TrLimits::sbump`]).
    pub ubump: u32,
    /// N3a: record without `RETF` (an earlier recording from this exit that used it failed).
    pub no_retf: bool,
}

/// S1: one inlined frame of the exit a side recording starts in (the fields of `exit::FrameRestore` the recorder needs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SideFrame {
    /// Root-relative slot of the frame's function slot.
    pub func_slot: u16,
    /// Results the call wants (`C - 1`).
    pub nresults: i16,
    /// The caller's `CALL` pc.
    pub call_pc: u32,
    /// Entered by a `TAILCALL`.
    pub tail: bool,
    /// A metamethod frame run in place (`CIST_MMRET`).
    pub mm: bool,
}

/// F2a: the entry address of the root trace of head `pc` of `pr` (0: none) and the address of the head's `entry` cell
/// (the link target of a prelude, TR8: the cell, not the address, is embedded in the code).
pub fn head_entry(st: &mut State, pr: ProtoRef, pc: u32) -> Option<(usize, u64)> {
    with_tr(st, pr, |tp| {
        tp.head_index(pc).map(|i| {
            let h = tp.head(i);
            (h.entry.get(), &h.entry as *const Cell<usize> as u64)
        })
    })
    .flatten()
}

static DEFAULT_START_SIDE: OnceLock<StartSideFn> = OnceLock::new();

/// Installs the process-wide side-recorder entry (first registration wins).
pub fn install_side_recorder(f: StartSideFn) -> bool {
    DEFAULT_START_SIDE.set(f).is_ok()
}

/// Registers the side-recorder entry for this state (overrides the process-wide one; tests).
pub fn set_side_starter(st: &mut State, f: Option<StartSideFn>) {
    d_of(st).tr.side_starter = f;
}

/// The prototype's rule for an exit site of trace `id` that reached the exit threshold (3.3): a type-guard site flushes
/// the trace (deferred, TR18) and re-records from the observed types; other sites are only counted. Kept for the
/// runtime tests and as the `maxside = 0` policy; the exit handler calls [`exit_hot_policy`]. Returns whether a flush
/// was requested.
pub fn exit_hot(st: &mut State, id: TrId, site: ExitSite) -> bool {
    let d = d_of(st);
    if !matches!(
        d.tr.slot(id),
        Some(Slot {
            body: Body::Trace(_),
            ..
        })
    ) {
        return false;
    }
    match site {
        ExitSite::TypeGuard => {
            d.tr.stats.exit_type_hot += 1;
            request_flush(st, id, FlushWhy::Rerec);
            true
        }
        ExitSite::Other | ExitSite::Alloc => {
            d.tr.stats.exit_other_hot += 1;
            false
        }
    }
}

/// An exit of trace `id` reached the hot threshold (called by the exit handler; `ci` is the root frame, `out` what the
/// restore did, `meta` the trace's exit metadata). The policy of 3.3 / 7.9 in the full build:
///
/// * a hot exit of a frameless snapshot gets a **side trace**: the handler has restored the exit to the stack, a
///   recording is armed at the snapshot's pc with the snapshot's slot types as known types, and the handler leaves for
///   the interpreter ([`HotAction::Interp`]); when the recording installs, `exit_cell[k]` points at the new trace;
/// * a type-guard exit of a trace that has no pre-roll to protect it against a ping-pong between a failing root and its
///   own side trace (a rolled root, or any side trace), or of a snapshot with inlined frames, keeps the prototype's rule:
///   flush the family and re-record from the observed types;
/// * other exits of snapshots with inlined frames are only counted;
/// * `maxside = 0` restores the prototype's policy exactly.
/// H1: is the family of trace `fam` rooted at a function-entry or continuation head (the exits of whose inlined frames got no side
/// traces before)?
fn fc_family(d: &JitData, fam: TrId) -> bool {
    match d.tr.slot(fam) {
        Some(Slot {
            body: Body::Trace(f),
            ..
        }) => tr_of(d, f.root).is_some_and(|tp| {
            let pc = tp.head(f.head).pc;
            tp.is_fn_head(pc) || tp.is_cont_head(pc)
        }),
        _ => false,
    }
}

pub fn exit_hot_policy(
    st: &mut State,
    ci: Ci,
    id: TrId,
    out: &super::exit::CoreOut,
    meta: &ExitMeta,
) -> HotAction {
    let d = d_of(st);
    let Some(Slot {
        body: Body::Trace(t),
        ..
    }) = d.tr.slot(id)
    else {
        return HotAction::None;
    };
    let ex = meta.exits[out.exitno as usize];
    let snap = &meta.snaps[ex.snap as usize];
    let hot = d.tr.cfg.hot_exit;
    // S1: an exit inside plain inlined frames of a loop-root family (the frames stay virtual in the side trace)
    let frame_sides = snap.frame_side_ok()
        && meta.frame_cells
        && !match d.tr.slot(t.family) {
            _ if !mutants::active("fcfn_family_excluded") => false,
            Some(Slot {
                body: Body::Trace(f),
                ..
            }) => tr_of(d, f.root).is_some_and(|tp| {
                let pc = tp.head(f.head).pc;
                tp.is_fn_head(pc) || tp.is_cont_head(pc)
            }),
            _ => true,
        }
        && !mutants::active("frame_side_never");
    let fc_new = frame_sides && fc_family(d, t.family);
    let sides = d.tr.cfg.limits.maxside > 0
        && (snap.frames.is_empty() || frame_sides)
        && snap.top.is_none()
        && snap.usable
        && !meta.cells.is_null()
        && !t.exit_st.is_empty();
    let family = t.family;
    if out.count == hot {
        let (e, sd) = (out.entry, sides);
        vlog(d, || {
            format!(
                "exit {} count {} hot: entry {e} sides {sd} site {:?} (side {} peeled {} frames {} top {} usable {} frame_sides {frame_sides})",
                out.exitno,
                out.count,
                out.site,
                t.side.is_some(),
                meta.peeled,
                snap.frames.len(),
                snap.top.is_some(),
                snap.usable
            )
        });
    }
    match out.site {
        ExitSite::TypeGuard => {
            let protected = (t.side.is_none() && meta.peeled)
                || (d.tr.cfg.limits.typesides != 0 && !mutants::active("typesides_ignored"));
            if !(sides && protected) {
                if out.count >= hot {
                    d.tr.stats.exit_type_hot += 1;
                    request_flush(st, family, FlushWhy::Rerec);
                    return HotAction::Flushed;
                }
                return HotAction::None;
            }
        }
        ExitSite::Other => {
            if out.count == hot {
                d.tr.stats.exit_other_hot += 1;
            }
            if !sides {
                return HotAction::None;
            }
        }
        ExitSite::Alloc => {
            if out.count == hot {
                d.tr.stats.exit_other_hot += 1;
            }
            return HotAction::None; // (P-B) the side trace would exit at the same allocation again
        }
    }
    let want = if fc_new && !mutants::active("fck_ignored") {
        hot.saturating_mul(d.tr.cfg.limits.fck)
    } else {
        hot
    };
    if out.count != want {
        return HotAction::None;
    }
    // N3a (RETF): the room the parent's entry checks promise is relative to the ENTRY frame; a child of a segment-k exit starts in the frame
    // popped `seg_pops` times, `seg_shift` slots lower, with that many more CallInfo nodes behind it
    let (pext, pdep) = child_room(meta.extent, meta.depth, snap.seg_shift, snap.seg_pops);
    side_attempt(
        st,
        ci,
        id,
        ex.snap,
        snap,
        out.entry,
        frame_sides,
        pext,
        pdep,
    )
}

/// N3a (RETF): the room (stack extent, spare `CallInfo` nodes) a side trace attached to an exit of segment `k` (the entry frame popped `seg_pops` times, `seg_shift`
/// slots lower) may count on, given the parent's entry-relative `extent` / `depth`: the child's coordinates start `seg_shift` slots below the entry frame, and its
/// root has `seg_pops` more `CallInfo` nodes behind it (the nodes of the popped frames exist).
pub fn child_room(extent: u16, depth: u8, seg_shift: u16, seg_pops: u8) -> (u16, u8) {
    if seg_pops == 0 {
        return (extent, depth);
    }
    (
        if mutants::active("retf_extent_unshifted") {
            extent
        } else {
            extent.saturating_add(seg_shift)
        },
        if mutants::active("retf_maxci_unshifted") {
            depth
        } else {
            depth.saturating_add(seg_pops)
        },
    )
}

fn side_attempt(
    st: &mut State,
    ci: Ci,
    id: TrId,
    k: u32,
    snap: &super::exit::ExitSnap,
    entry: bool,
    frame_side: bool,
    pextent: u16,
    pdepth: u8,
) -> HotAction {
    let d = d_of(st);
    let link = SideLink {
        parent: id,
        exit: k,
        prelude: false,
    };
    let mut deferred_mark: Option<u32> = None;
    let (state, nside, prelude) = match d.tr.slot(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => {
            let nside = match d.tr.slot(t.family) {
                Some(Slot {
                    body: Body::Trace(f),
                    ..
                }) => f.nside,
                _ => 0,
            };
            // The exit is the root's entry check and fails about as often as the root is entered (type_entry: every
            // run of the loop fails once and then enters): the side trace becomes the head's prelude, so that the
            // entries it serves do not pass through the failing root entry first. (`entry_ok` counts the successful
            // entries so far, `hot_exit` the failures: at most twice as many successes as failures.)
            let cand = (entry && t.side.is_none() && !mutants::active("prelude_never"))
                .then(|| {
                    tr_of(d, t.root).and_then(|tp| {
                        let h = tp.head(t.head);
                        (h.state.get() == HeadState::Traced
                            && h.trace.get() == Some(id)
                            && h.pre.get() == 0)
                            .then(|| h.entry_ok.get())
                    })
                })
                .flatten();
            // The first time the evidence is the whole history (successes since the root was installed, the loop may
            // have been entered without failing for a long time first: type_entry's first ~200 runs); when it speaks
            // against a prelude the decision waits for one more window of `hot_exit` failures and then looks only at
            // the successes in between.
            let (prelude, defer) = match (cand, t.mark.get(k as usize).copied().flatten()) {
                (Some(ok), None) if u64::from(ok) <= 2 * u64::from(d.tr.cfg.hot_exit) => {
                    (true, None)
                }
                (Some(ok), None) => (false, Some(ok)),
                (Some(ok), Some(m)) => (
                    u64::from(ok.saturating_sub(m)) <= 2 * u64::from(d.tr.cfg.hot_exit),
                    None,
                ),
                (None, _) => (false, None),
            };
            if let Some(ok) = defer {
                // (below, once the borrow of the trace ends)
                deferred_mark = Some(ok);
            }
            vlog(d, || {
                format!(
                    "side attempt exit {k}: entry {entry} root {} prelude {prelude} head {:?}",
                    t.side.is_none(),
                    tr_of(d, t.root).map(|tp| {
                        let h = tp.head(t.head);
                        (
                            h.state.get(),
                            h.trace.get() == Some(id),
                            h.pre.get(),
                            h.entry_ok.get(),
                        )
                    })
                )
            });
            (t.exit_st.get(k as usize).copied(), nside, prelude)
        }
        _ => return HotAction::None,
    };
    if state != Some(SideSt::Free) {
        return HotAction::None;
    }
    if d.tr.cfg.limits.loopanymax != 0
        && !snap.frames.is_empty()
        && !mutants::active("loopany_guard_never")
    {
        let sites = family_la(d, id);
        if !sites.is_empty() {
            let (fam, max) = match d.tr.slot(id) {
                Some(Slot {
                    body: Body::Trace(t),
                    ..
                }) => (t.family, d.tr.cfg.limits.loopanymax),
                _ => return HotAction::None,
            };
            let n = match d.tr.slot_mut(fam) {
                Some(Slot {
                    body: Body::Trace(f),
                    ..
                }) => {
                    f.la_sides += 1;
                    f.la_sides
                }
                _ => 0,
            };
            if n > max {
                vlog(d, || {
                    format!("loopany guard: family {fam:?} had {n} side attempts inside callee frames: sites blocked")
                });
                let root_unrolled = matches!(d.tr.slot(fam), Some(Slot { body: Body::Trace(f), .. }) if !f.la.is_empty());
                let plan = la_plan(d.tr.cfg.limits.laprune, root_unrolled);
                let mut victims: Vec<TrId> = Vec::new();
                if plan == LaPlan::Sides {
                    for (i, sl) in d.tr.slots.iter().enumerate() {
                        if let Body::Trace(x) = &sl.body {
                            if la_is_victim(x.family == fam, x.side.is_some(), !x.la.is_empty()) {
                                victims.push(TrId {
                                    slot: i as u32,
                                    gen: sl.gen,
                                });
                            }
                        }
                    }
                }
                for s in sites {
                    super::record::la_block(s);
                }
                d.tr.stats.la_blocked += 1;
                if plan == LaPlan::Sides {
                    vlog(d, || {
                        format!("loopany guard: root of {fam:?} unrolled nothing: pruned side traces {victims:?}, root kept")
                    });
                    if let Some(Slot {
                        body: Body::Trace(f),
                        ..
                    }) = d.tr.slot_mut(fam)
                    {
                        f.la_sides = 0;
                    }
                    if !mutants::active("laprune_never_flushes_sides") {
                        d.tr.stats.la_pruned += victims.len() as u64;
                        d.tr.stats.la_self += u64::from(victims.contains(&id));
                        // (a victim may be the trace whose exit is being handled: `request_flush` defers the retire while the handler runs;
                        // a victim inside the subtree of an earlier victim is stale by then and ignored)
                        for v in victims {
                            request_flush(st, v, FlushWhy::Dep);
                        }
                    }
                } else {
                    d.tr.stats.la_family += 1;
                    request_flush(st, fam, FlushWhy::Ineffective);
                }
                return HotAction::None;
            }
        }
    }
    if let Some(ok) = deferred_mark {
        if let Some(Slot {
            body: Body::Trace(t),
            ..
        }) = d.tr.slot_mut(id)
        {
            t.mark[k as usize] = Some(ok);
        }
        side_failed(d, link, false, "Defer", false);
        return HotAction::None;
    }
    // BUNDLE-G (`chaindepth`): a callee-guard exit at the end of a chain of `chaindepth` links gets no further link
    let cd = d.tr.cfg.limits.chaindepth;
    if cd != 0 && chain_refuse(d, id, k as usize, cd) {
        d.tr.stats.chain_deep += 1;
        side_failed(d, link, true, "ChainDepth", true);
        if let Some(Slot {
            body: Body::Trace(t),
            ..
        }) = d.tr.slot_mut(id)
        {
            black_exit(&mut t.exit_st, &t.exits, k as usize);
        }
        return HotAction::None;
    }
    let mut capped = nside >= d.tr.cfg.limits.maxside && !mutants::active("side_maxside_ignored");
    if capped {
        // BUNDLE-O: the family is full, but an exit that keeps coming back may still get a side trace up to `maxside + polyside`: it is
        // deferred (re-armed, no fault) until it has been reported `polywait` times
        let (poly, wait, cap) = {
            let l = &d.tr.cfg.limits;
            (l.polyside, l.polywait, l.maxside.saturating_add(l.polyside))
        };
        // only exits of a dispatch guard (callee identity, hash node / metatable of a method or field lookup): a polymorphic site; the cold
        // exits of branches / bounds / type checks keep the hard cap (awfy json / queens spent their extra side traces for nothing)
        let deep = snap.frames.len() as u32 >= d.tr.cfg.limits.polyframes
            || mutants::active("polyframes_ignored");
        let hot_family = poly > 0 && nside < cap && deep;
        if hot_family {
            let waited = match d.tr.slot_mut(link.parent) {
                Some(Slot {
                    body: Body::Trace(t),
                    ..
                }) if (k as usize) < t.defer.len() => {
                    let c = &mut t.defer[k as usize];
                    *c = c.saturating_add(1);
                    u32::from(*c) >= wait || mutants::active("polywait_ignored")
                }
                _ => false,
            };
            if waited {
                vlog(d, || {
                    format!("polyside: exit {k} (pc {} frames {}) of {:?} beyond the cap ({nside} side traces in the family)", snap.pc, snap.frames.len(), link.parent)
                });
                capped = false;
            } else {
                side_failed(d, link, false, "PolyWait", false);
                return HotAction::None;
            }
        }
    }
    if capped {
        // the family is full: this exit stays in tier 1 (7.9)
        side_failed(d, link, true, "MaxSide", true);
        if let Some(Slot {
            body: Body::Trace(t),
            ..
        }) = d.tr.slot_mut(id)
        {
            black_exit(&mut t.exit_st, &t.exits, k as usize);
        }
        return HotAction::None;
    }
    // another recording is active, a hook is installed, a global penalty is pending: not now, try again after
    // `hot_exit` more exits (no fault of the exit)
    if d.tr.active.is_some()
        || jit_hooks::rec_active()
        || st.hook_mask() != 0
        || d_of(st).tr.global_pen > 0
    {
        side_failed(d_of(st), link, false, "Busy", false);
        return HotAction::None;
    }
    let Some(start) = d_of(st)
        .tr
        .side_starter
        .or_else(|| DEFAULT_START_SIDE.get().copied())
    else {
        return HotAction::None;
    };
    if frame_side && snap.frame_side_ok() && !snap.frames.is_empty() {
        let sf: Vec<SideFrame> = snap
            .frames
            .iter()
            .map(|f| SideFrame {
                func_slot: f.func_slot,
                nresults: f.nresults,
                call_pc: f.call_pc,
                tail: f.tail,
                mm: f.mm,
            })
            .collect();
        if !super::record::side_frames_supported(st, &sf) {
            // (a metamethod frame of an instruction the recorder does not inline: the path ends the same way next time)
            side_failed(d_of(st), link, true, "FrameKind", true);
            return HotAction::None;
        }
    }
    let Ok(rid) = begin_side_recording_ex(st, id, k, prelude) else {
        side_failed(d_of(st), link, false, "Refused", false);
        return HotAction::None;
    };
    // S1: the frames stay virtual; slot numbers are root-relative in every frame, so the exit stub's slots are known
    // for a side trace with frames as well.
    let virt = frame_side && snap.frame_side_ok();
    vlog(d_of(st), || {
        format!(
            "side start at exit {k}: frames {} (mm {}) virtual {virt}",
            snap.frames.len(),
            snap.frames.iter().filter(|f| f.mm).count()
        )
    });
    let known: Vec<(u16, super::ir::Ty)> = if snap.frames.is_empty() || virt {
        snap.slots
            .iter()
            .filter_map(|s| super::exit::ty_of_tag(s.tag).map(|t| (s.slot, t)))
            .collect()
    } else {
        Vec::new()
    };
    let ci = if snap.frames.is_empty() || virt {
        ci
    } else {
        st.ci()
    };
    let frames: Vec<SideFrame> = if virt {
        snap.frames
            .iter()
            .map(|f| SideFrame {
                func_slot: f.func_slot,
                nresults: f.nresults,
                call_pc: f.call_pc,
                tail: f.tail,
                mm: f.mm,
            })
            .collect()
    } else {
        Vec::new()
    };
    let ubump = {
        let d = d_of(st);
        let (last, fails) = match d.tr.slot(id) {
            Some(Slot {
                body: Body::Trace(t),
                ..
            }) => (
                t.last.get(k as usize).copied().flatten(),
                t.fails.get(k as usize).copied().map_or(0, u32::from),
            ),
            _ => (None, 0),
        };
        let retry = matches!(last, Some("SideShort" | "Recursion"));
        let base = if d.tr.cfg.limits.sbump != 0
            && retry
            && !frames.is_empty()
            && !mutants::active("sbump_never")
        {
            d.tr.cfg.limits.sbump
        } else {
            0
        };
        base + sbump2_extra(d.tr.cfg.limits.sbump2, last, fails, !frames.is_empty())
    };
    let no_retf = matches!(d_of(st).tr.slot(id), Some(Slot { body: Body::Trace(t), .. }) if t.noretf.get(k as usize).copied().unwrap_or(false));
    match start(
        st,
        ci,
        rid,
        &SideStart {
            pc: snap.pc,
            known,
            prelude,
            frames,
            max_extent: pextent,
            max_ci: pdepth,
            ubump,
            no_retf,
        },
    ) {
        Some(rem) => HotAction::Interp(rem),
        None => {
            vlog(d_of(st), || {
                format!(
                    "side start refused at exit {k} (frames {})",
                    snap.frames.len()
                )
            });
            abort_transient(st, rid, "StartRefused");
            HotAction::None
        }
    }
}

/// F2b: counts an installed nest root (a root trace that ends with a link).
pub fn note_side_frame_link(st: &mut State) {
    d_of(st).tr.stats.side_frame_links += 1;
}

pub fn note_nest(st: &mut State, frame: bool) {
    if frame {
        d_of(st).tr.stats.frame_links += 1;
    } else {
        d_of(st).tr.stats.nest_installed += 1;
    }
}

/// J1-1: counts the fused `HREFK`+`HLOAD` pairs of an installed trace.
pub fn note_hk_fused(st: &mut State, n: u32, split: u32) {
    d_of(st).tr.stats.hk_fused += u64::from(n);
    d_of(st).tr.stats.hk_split += u64::from(split);
}

/// F2b: counts an installed stitching trace.
pub fn note_stitch(st: &mut State) {
    d_of(st).tr.stats.stitches += 1;
}

/// P-M3: is the head at `pc` of `pr` a continuation head?
pub fn head_is_cont(st: &mut State, pr: ProtoRef, pc: u32) -> bool {
    let d = d_of(st);
    tr_of(d, pr.0).is_some_and(|tp| tp.is_cont_head(pc))
}

/// R2: `RUSTYNAIL_JIT_VERBOSE=3` prints the pc and opcode of every recording abort (a debugging aid).
pub fn verbose_aborts() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("RUSTYNAIL_JIT_VERBOSE").is_ok_and(|v| v == "3"))
}

/// J2-2: is the recording `id` (root or side) one of a family whose root head is recorded with the deep limits?
pub fn rec_deep(st: &mut State, id: TrId) -> bool {
    let d = d_of(st);
    let Some(Slot {
        body: Body::Rec(r), ..
    }) = d.tr.slot(id)
    else {
        return false;
    };
    let (root, head) = (r.root, r.head);
    let deep = tr_of(d, root).is_some_and(|tp| tp.head_deep(head));
    if deep {
        d.tr.stats.deep_recs += 1;
    }
    deep
}

/// R2: a native self-call of `pr` was recorded.
pub fn mark_recursive(st: &mut State, pr: ProtoRef) {
    let d = d_of(st);
    if let Some(tp) = tr_of(d, pr.0) {
        tp.recursive.set(true);
    }
}

/// BUNDLE-K L1: are the traces of `pr` lazy-aware?
pub fn proto_lazy(st: &mut State, pr: ProtoRef) -> bool {
    let d = d_of(st);
    tr_of(d, pr.0).is_some_and(|tp| tp.lazy.get())
}

/// R2: has a native self-call of `pr` been recorded?
pub fn proto_recursive(st: &mut State, pr: ProtoRef) -> bool {
    let d = d_of(st);
    tr_of(d, pr.0).is_some_and(|tp| tp.recursive.get())
}

/// P-E2: is the head at `pc` of `pr` the function-entry head?
pub fn head_is_fn(st: &mut State, pr: ProtoRef, pc: u32) -> bool {
    let d = d_of(st);
    tr_of(d, pr.0).is_some_and(|tp| tp.is_fn_head(pc))
}

/// F2a: is the head at `pc` of `pr` a loop head whose root trace is installed (`Some(true)`), a head without one
/// (`Some(false)`), or not a head (`None`)? The side recorder's question when it reaches a pc of the root frame.
/// (The function-entry head of P-E2 is not a loop head: `None`.)
pub fn head_traced(st: &mut State, pr: ProtoRef, pc: u32) -> Option<bool> {
    with_tr(st, pr, |tp| {
        if tp.is_fn_head(pc) || tp.is_cont_head(pc) {
            return None;
        }
        tp.head_index(pc)
            .map(|i| tp.head(i).state.get() == HeadState::Traced)
    })
    .flatten()
}

/// The entry-failure counters of head `pc` reached the threshold (`jit_tr_entry_fail`): queue the flush and
/// re-record. Returns whether a flush was queued.
pub fn entry_fail_hot(st: &mut State, pr: ProtoRef, pc: u32) -> bool {
    let d = d_of(st);
    let Some(tp) = tr_of(d, pr.0) else {
        return false;
    };
    let Some(i) = tp.head_index(pc) else {
        return false;
    };
    if !tp.entry_fail_hot(i) {
        return false;
    }
    match tp.head(i).trace.get() {
        Some(id)
            if matches!(
                d.tr.slot(id),
                Some(Slot {
                    body: Body::Trace(_),
                    ..
                })
            ) =>
        {
            // P3 (`ncallee`): a persistent entry failure of a root whose entry segment holds a callee identity guard: the callee changes between entries (a call
            // through a table of functions). The re-record keeps the callee guards in the body, where a mismatch is an ordinary exit with a side trace.
            let ec =
                matches!(d.tr.slot(id), Some(Slot { body: Body::Trace(t), .. }) if t.entry_callee);
            if ncallee_due(d.tr.cfg.limits.ncallee, ec) {
                if !tp.head(i).ncallee.replace(true) {
                    d.tr.stats.ncallee_heads += 1;
                }
                vlog(d, || {
                    format!("entry-fail flush of a root with a callee guard in its entry segment: head pc {pc} records its callee guards in the body")
                });
            }
            request_flush(st, id, FlushWhy::Rerec);
            true
        }
        _ => false,
    }
}

/// Pre-roll exits of trace `id` per entry at which it counts as ineffective: `exits * 10 >= entries * 9`.
pub const INEFF_RATIO_PCT: u64 = 90;
/// F2a: the same ratio for a root that has a side trace on a type-guard exit (see [`pre_exit_check`]).
pub const SIDE_INEFF_RATIO_PCT: u64 = 50;
/// P2: share of all pre-roll exits that one type-guard site with a side trace must take for the root to count as a persistent
/// type change (`ineffcover`).
pub const PERSIST_PCT: u64 = 80;
/// P2: share of the entries that leave through a pre-roll exit (covered by a side trace or not) from which a root is ineffective
/// anyway (`ineffcover`).
pub const COVER_PCT: u64 = 90;
/// P2: consecutive checks (1024 pre-roll exits apart) at which the entry rate must stay below the limit.
pub const HOP_STREAK: u32 = 4;

/// P-B: consecutive allocation exits (`ExitSite::Alloc`) of one family that follow each other within [`STORM_INSNS`] instructions
/// before the loop is given back to tier 1.
pub const STORM_STREAK: u32 = 64;
/// P-B: two allocation exits closer than this many executed instructions belong to a storm (the trace is left about once per
/// iteration; every exit costs more than the iteration it interrupts).
pub const STORM_INSNS: i64 = 200;
/// N3b: bytes the collector leaves between two steps at or above which allocation exits are never a storm (see [`alloc_storm`]).
pub const STORM_ROOM: usize = 512;

thread_local! {
    static STORM: RefCell<BTreeMap<(u32, u32), (i32, u32)>> = const { RefCell::new(BTreeMap::new()) };
}

/// P-B: the counters of the storm rule for one family: `(budget word at the previous allocation exit, exits in a row within
/// [`STORM_INSNS`])`. Records an exit at `budget`; true when [`STORM_STREAK`] in a row have been seen (the streak starts again).
pub fn storm_step(e: &mut (i32, u32), budget: i32) -> bool {
    let delta = i64::from(e.0) - i64::from(budget);
    e.0 = budget;
    if (0..=STORM_INSNS).contains(&delta) {
        e.1 += 1;
    } else {
        e.1 = 0;
    }
    if e.1 >= STORM_STREAK && !mutants::active("storm_rule_skip") {
        e.1 = 0;
        true
    } else {
        false
    }
}

/// P-B: an allocation exit of trace `id` happened with the instruction budget word at `budget`. After [`STORM_STREAK`] such exits in a
/// row, each within [`STORM_INSNS`] instructions of the previous one (the collector steps behind nearly every allocation of the
/// loop, a trace exits at every iteration and runs slower than tier 1), the family is flushed like an ineffective trace
/// (`rerec += 1`: a few re-records, then the head is blacklisted and tier 1 owns it). Deterministic: it depends on instruction counts only.
/// Returns whether a flush was requested.
pub fn alloc_storm(st: &mut State, id: TrId, budget: i32) -> bool {
    // N3b: the budget word is refilled whenever the interpreter runs, so exits that are exactly periodic (every n iterations) show the
    // same word each time (a delta of 0) however large n is. The room the collector leaves between two steps tells a storm from that:
    // with more than `STORM_ROOM` bytes between steps a trace that allocates a few dozen bytes per iteration runs several iterations
    // per entry.
    if st.g().heap.jit_gc_step_hint() >= STORM_ROOM {
        STORM.with(|m| m.borrow_mut().clear());
        return false;
    }
    let family = match d_of(st).tr.slot(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => t.family,
        _ => return false,
    };
    let flush = STORM.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 256 {
            m.clear();
        }
        storm_step(
            m.entry((family.slot, family.gen)).or_insert((budget, 0)),
            budget,
        )
    });
    if flush {
        request_flush(st, family, FlushWhy::Ineffective);
    }
    flush
}

/// H3-G: the exit handler materialised a sunk table of a demoted shape: the family of trace `id` is flushed (re-recorded without sinking
/// that shape). Returns whether a flush was requested.
pub fn sink_demoted(st: &mut State, id: TrId) -> bool {
    let family = match d_of(st).tr.slot(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => t.family,
        _ => return false,
    };
    request_flush(st, family, FlushWhy::Unsink);
    true
}

/// The ineffective-trace rule (Q2) for a root whose pre-roll sites took `p` exits while `e` entries passed the entry
/// segment: `p >= 90% of e`, or `>= 50%` when `guarded` (a side trace hangs off a type-guard exit of the root: F2a).
pub fn pre_exit_flush_due(p: u64, e: u64, guarded: bool) -> bool {
    pre_exit_flush_due_ex(p, e, guarded, false)
}

/// N5: the lowered (F2a) ratio only applies while the entries that pass the pre-roll are rare: `n` = exits through
/// non-pre-roll sites (loop-body guards, taken by entries that completed at least one iteration), `p` = pre-roll exits. A
/// loop whose guard fails every other iteration hops root -> side trace -> root and runs several iterations per hop
/// (`n` about `p`): that costs less than tier 1 (bench/lj/cliff_mixed_strings.lua: 897 -> 80 Mcyc with the rule off) and is
/// not flushed. A persistent change that fails in the pre-roll alone (`n` near 0) still is (type_late, sieve).
pub fn side_ratio_applies(p: u64, n: u64) -> bool {
    n * 2 < p
}

/// P-E2: ratio at which a function-entry root is ineffective: an early exit costs a materialised frame chain and a
/// handler call, which one completed entry has to repay (measured on `fib` / `rec_loop`, docs/JIT_TRACE_REF.md P-E2).
pub const FN_INEFF_RATIO_PCT: u64 = 20;

/// [`pre_exit_flush_due`] with the function-entry root rule.
pub fn pre_exit_flush_due_ex(p: u64, e: u64, guarded: bool, fn_root: bool) -> bool {
    pre_exit_flush_due_n(p, e, guarded, fn_root, 0)
}

/// [`pre_exit_flush_due_ex`] with the body-exit count `n` of N5 ([`side_ratio_applies`]).
pub fn pre_exit_flush_due_n(p: u64, e: u64, guarded: bool, fn_root: bool, n: u64) -> bool {
    let pct = if fn_root && !mutants::active("fn_ineff_pct_ignored") {
        FN_INEFF_RATIO_PCT
    } else if guarded && side_ratio_applies(p, n) && !mutants::active("ineffective_pct_ignored") {
        SIDE_INEFF_RATIO_PCT
    } else {
        INEFF_RATIO_PCT
    };
    p * 100 >= e * pct
}

/// The ineffective-trace rule (Q2). Called by the exit handler each time a pre-roll site of `id` reached another
/// multiple of [`exit::INEFF_EXITS`]: `P` = exits of all pre-roll sites of the trace (counters of `meta`), `E` =
/// `entry_ok` of its head (entries that passed the entry segment since the trace was installed). `P >= 90% of E` (and
/// `P >= INEFF_EXITS`, implied by the call condition) means nearly every entry leaves before completing one iteration
/// and every back-edge of the head pays an entry + exit for nothing: the trace is flushed like a persistently failing
/// one (`rerec += 1`, so after [`MAX_RERECORD`] re-records the head is blacklisted and tier 1 owns it). The decision
/// depends only on counters, hence is deterministic. Returns whether a flush was queued.
pub fn pre_exit_check(st: &mut State, ci: Ci, id: TrId, meta: &ExitMeta) -> bool {
    if mutants::active("ineffective_never") || meta.counters.is_null() {
        return false;
    }
    let mut p: u64 = 0;
    let mut n: u64 = 0;
    // P2 (`ineffcover`): `pu` = pre-roll exits that left the trace (no side trace attached to the site), `top` = the largest
    // pre-roll count of one type-guard site that has a side trace. Exits that continue in a side trace are coverage of the tree;
    // only a persistent type change (one such site takes `PERSIST_PCT` of all pre-roll exits) keeps the ping-pong rule of F2a.
    let cover = d_of(st).tr.cfg.limits.ineffcover != 0 && !mutants::active("ineff_cover_ignored");
    let sides: Vec<SideSt> = match d_of(st).tr.slot(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => t.exit_st.clone(),
        _ => Vec::new(),
    };
    let mut pu: u64 = 0;
    let mut top: u64 = 0;
    let mut top_any: u64 = 0;
    for (k, e) in meta.exits.iter().enumerate() {
        // SAFETY: `counters` points at `exits.len()` cells owned by the (live) trace (ExitMeta contract).
        let c = u64::from(exit_count(unsafe { &*meta.counters.add(k) }.get()));
        if e.pre {
            p += c;
            if matches!(sides.get(k), Some(SideSt::Has(_))) {
                top_any = top_any.max(c);
                if e.site == ExitSite::TypeGuard {
                    top = top.max(c);
                }
            } else {
                pu += c;
            }
        } else {
            n += c; // N5: body exits (includes entry-check exits, which a persistent failure flushes by its own rule)
        }
    }
    let persistent =
        !cover || (top * 100 >= p * PERSIST_PCT && !mutants::active("ineff_persist_never"));
    let p_all = p;
    if cover && !persistent {
        p = pu; // polymorphic: the side-trace hops are coverage, only the exits that left the trace count
    }
    let Some(pr) = ci_proto_ref(st, ci) else {
        return false;
    };
    let d = d_of(st);
    if !matches!(
        d.tr.slot(id),
        Some(Slot {
            body: Body::Trace(_),
            ..
        })
    ) {
        return false;
    }
    let Some(tp) = tr_of(d, pr.0) else {
        return false;
    };
    let Some(h) = tp.heads().iter().find(|h| h.trace.get() == Some(id)) else {
        return false;
    };
    let e = u64::from(h.entry_ok.get());
    // F2a: a root with a side trace on a type-guard exit can ping-pong with it (every entry fails the guard, the side
    // trace links back into the entry): the threshold is lower, so that a persistent type change is re-recorded as
    // the prototype did (a hot type-guard exit flushed the trace at once)
    let guarded = match d.tr.slot(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => t.exit_st.iter().enumerate().any(|(k, s)| {
            matches!(s, SideSt::Has(_))
                && meta
                    .exits
                    .get(k)
                    .is_some_and(|e| e.site == ExitSite::TypeGuard)
        }),
        _ => false,
    };
    vlog(d, || {
        format!(
            "pre-roll check trace slot {} p {p} e {e} n {n} guarded {guarded}",
            id.slot
        )
    });
    vlog(d, || {
        let mut o = String::new();
        for (k, x) in meta.exits.iter().enumerate() {
            // SAFETY: as above
            let c = exit_count(unsafe { &*meta.counters.add(k) }.get());
            if c > 0 {
                let st = match d.tr.slot(id) {
                    Some(Slot {
                        body: Body::Trace(t),
                        ..
                    }) => format!("{:?}", t.exit_st.get(k)),
                    _ => String::new(),
                };
                o += &format!(" [{k}: pre {} {:?} {st} x{c}]", x.pre, x.site);
            }
        }
        format!("pre-roll sites{o}")
    });
    // P2: coverage does not excuse a root whose entries are too short to repay the hop: when (nearly) every entry leaves the root's
    // pre-roll for a side trace, root, hop and the return to the head are pure overhead if the entries come at a high rate
    // (oop_methods: entry -> exit -> side -> entry per iteration, ~100 cycles apart; richards / deltablue: thousands of cycles of
    // work in the tree per entry). The rate is measured between two checks of the same trace with the time-stamp counter.
    let mut hopping = false;
    if hop_rule_applies(
        cover,
        tp.head_deep(tp.head_index(h.pc).unwrap_or(0)) && d.tr.cfg.limits.deephop != 0,
    ) && p_all * 100 >= e * COVER_PCT
    {
        // BUNDLE-P: the default rule is deterministic (counters only): a root through which `hoptop` percent of the entries leave by ONE pre-roll
        // site, into the same side trace, never runs its own loop (a trampoline: the loop runs in the side trace, without the root's loop
        // optimisations).
        let lim = &d.tr.cfg.limits;
        let lim_top = u64::from(lim.hoptop);
        let m = &mut d.tr.hops;
        if m.len() > 256 {
            m.clear();
        }
        let key = (id.slot, id.gen);
        let prev = m.get(&key).copied();
        let (mut streak, mut due) = (0u32, false);
        let mut dom_only = false;
        if let Some(p0) = prev {
            let by_top =
                lim_top != 0 && top_any * 100 >= e * lim_top && !mutants::active("hop_top_ignored");
            let by_dom = dom_due(lim.domtop, lim.domn, top_any, e, h.dom_done.get());
            dom_only = by_dom && !by_top;
            due = (by_top || by_dom) && !mutants::active("ineff_hop_never");
            streak = if due { p0.streak + 1 } else { 0 };
        }
        m.insert(key, HopSt { streak });
        // a transient fast phase (warm-up, a burst of short entries) must not flush a tree: the rate has to hold for HOP_STREAK checks
        hopping = due && streak >= HOP_STREAK;
        if hopping && dom_only {
            // C36: the root sits on a rare path of its loop: one re-record (once per head, so no re-record loop), nothing else changes
            h.dom_done.set(true);
            d.tr.stats.dom_flushes += 1;
            vlog(d_of(st), || {
                format!("dominant side: root slot {} had {}% of its {e} entries through one side trace: re-record once", id.slot, top_any * 100 / e.max(1))
            });
            request_flush(st, id, FlushWhy::Dom);
            return true;
        }
        vlog(d_of(st), || {
            format!("hop rate: top site {}% of the entries (limit {lim_top}) streak {streak} hopping {hopping}", top_any * 100 / e.max(1))
        });
    }
    if !hopping && !pre_exit_flush_due_n(p, e, guarded, tp.is_fn_head(h.pc), n) {
        return false;
    }
    // BUNDLE-L B (`bhoist`): a root with hoisted bounds guards that leaves before its body most of the time (the loop runs past the array part, or a
    // later check of the original body failed first): re-record once without the hoist (counter-based, once per head, no penalty).
    if matches!(d_of(st).tr.slot(id), Some(Slot { body: Body::Trace(t), .. }) if t.hoisted)
        && !h.nohoist.get()
    {
        h.nohoist.set(true);
        vlog(d_of(st), || {
            format!("bhoist: ineffective root slot {} with hoisted bounds guards: re-record without the hoist", id.slot)
        });
        return sink_demoted(st, id);
    }
    // SINK-R (`sinkroot`): a root whose leaving pre-roll exits name sunk tables is re-recorded once without sinking those shapes (Unsink: no penalty)
    // before it is flushed as ineffective. Counter-based and once per (shape, key): a shape already demoted falls through to the flush below.
    if d_of(st).tr.cfg.limits.sinkroot != 0 && !mutants::active("sinkroot_never") {
        let scoped = d_of(st).tr.cfg.limits.sinkroot == 2;
        let pk = rt::proto_of(st, ci).map_or(0, |cl| cl.proto().as_ptr() as u64);
        let mut any = false;
        for (k, ex) in meta.exits.iter().enumerate() {
            // SAFETY: as above.
            let c = exit_count(unsafe { &*meta.counters.add(k) }.get());
            if !ex.pre || c == 0 {
                continue;
            }
            for t in &meta.snaps[ex.snap as usize].sunk {
                let sh = super::sink::shape_of(t.b, t.c, t.fields.first().map_or(0, |f| f.key));
                let key = if scoped {
                    super::sink::scoped_shape(sh, pk, h.pc)
                } else {
                    sh
                };
                any |= super::sink::demote_shape(key);
            }
        }
        if any {
            vlog(d_of(st), || {
                format!("sinkroot: ineffective root slot {} with sunk pre-roll exits: re-record without sinking", id.slot)
            });
            return sink_demoted(st, id);
        }
    }
    request_flush(st, id, FlushWhy::Ineffective);
    true
}

/// J2-2: does the hop-rate rule of [`pre_exit_check`] apply to a root? It does with `ineffcover` (`cover`), except for a root recorded with the
/// deep limits: its tree is a fixed set of call shapes whose side-trace hops are inline stub jumps, and a deep tree done in ~100 cycles per entry
/// sat at the threshold of the removed clock rule (`hopcyc`, K1), where the timing of the run decided whether the whole family was flushed (rec_loop: 145 or
/// 610 Mcyc from run to run).
pub fn hop_rule_applies(cover: bool, deep: bool) -> bool {
    cover && !(deep && !mutants::active("deep_hop_rule_kept"))
}

/// [`entry_fail_hot`] for the head whose installed trace is `id` (the exit handler of an entry-check exit knows the
/// trace, not the head pc). `ci` is the root frame.
pub fn entry_fail_hot_id(st: &mut State, ci: Ci, id: TrId) -> bool {
    let Some(pr) = ci_proto_ref(st, ci) else {
        return false;
    };
    let d = d_of(st);
    let Some(tp) = tr_of(d, pr.0) else {
        return false;
    };
    let Some(pc) = tp
        .heads()
        .iter()
        .find(|h| h.trace.get() == Some(id))
        .map(|h| h.pc)
    else {
        return false;
    };
    entry_fail_hot(st, pr, pc)
}

// ---- free_code / state hooks ------------------------------------------------------------------------------

/// The prototype with descriptor `key` is being freed (called from `rt::free_code` after it left `live`):
/// flushes the traces rooted at it and those that depend on it, aborts recordings that depend on it (TR32), and
/// returns the bytes to refund.
pub(crate) fn on_proto_free(d: &mut JitData, key: usize, tp: &TrProto) -> usize {
    let mut refund = 0;
    let mut ids: Vec<TrId> = tp.deps.take();
    for h in tp.heads() {
        d.tr.stats.entered += u64::from(h.entry_ok.get());
        if let Some(id) = h.trace.get() {
            ids.push(id);
        }
    }
    for id in ids {
        match d.tr.slot(id).map(|s| matches!(s.body, Body::Rec(_))) {
            Some(true) => {
                let hook = d.tr.dep_hook;
                if let Some(Slot {
                    body: Body::Rec(r), ..
                }) = d.tr.slot_mut(id)
                {
                    r.dep_freed = true;
                    // the address may be reused by a descriptor compiled during the recording (TR4b)
                    if r.root == key {
                        r.root = 0;
                    }
                    r.deps.retain(|x| *x != key);
                }
                vlog(d, || format!("recording {} lost a dependency", id.slot));
                if let Some(f) = hook {
                    f(id);
                }
            }
            Some(false) => refund += flush_or_defer(d, id, FlushWhy::Dep),
            None => {}
        }
    }
    let _ = key;
    refund
}

/// Mutant `tailhook_ignored` (the injection site lives in `rt::hot_call`): the knob reads as off.
pub(crate) fn mutant_tailhook_ignored() -> bool {
    mutants::active("tailhook_ignored")
}

/// Mutant `retired_freed_early` (the injection site lives in `rt::recompile_inner`, which frees the retired block).
pub(crate) fn mutant_retired_freed_early() -> bool {
    mutants::active("retired_freed_early")
}

/// F5 (TR19b(c), `+verify`): every exit landing address of every live trace lies in a code block of a live prototype
/// (its current block or one of its retired versions). A retired block freed too early, or a trace that outlived the
/// prototype whose code it lands in, fails here.
pub(crate) fn verify_landings(d: &JitData) {
    if !d.tr.cfg.opts.verify {
        return;
    }
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &k in &d.live {
        // SAFETY: every key of `live` is a registered, not yet freed descriptor of this state.
        let jp = unsafe { &*(k as *const JitProto) };
        ranges.push((jp.block.addr(), jp.block.size()));
        if let Some(t) = jp.t15.as_deref() {
            ranges.extend(t.retired.iter().map(|b| (b.addr(), b.size())));
        }
    }
    for (slot, sl) in d.tr.slots.iter().enumerate() {
        let Body::Trace(t) = &sl.body else { continue };
        let Some(m) = &t.xmeta else { continue };
        for (k, sn) in m.snaps.iter().enumerate() {
            let a = sn.land_addr as usize;
            assert!(
                a == 0 || ranges.iter().any(|&(b, n)| a >= b && a < b + n),
                "trace slot {slot} (root {:#x} live {}, head {}, side {}): exit {k} lands at {a:#x}, in no live tier-1 block (a retired code version was freed too early); blocks {ranges:x?}",
                t.root,
                d.live.contains(&t.root),
                t.head,
                t.side.is_some()
            );
        }
    }
}

/// Traces were switched back on: every `Warm` head counts normally again (the off-path of [`hot`] and
/// `FlushWhy::Off` parked them at `2^30`).
pub(crate) fn rearm_all(d: &JitData) {
    if d.tr.cfg.mode == TrMode::Off || !d.enabled {
        return;
    }
    for &k in &d.live {
        if let Some(tp) = tr_of(d, k) {
            for (i, h) in tp.heads().iter().enumerate() {
                if h.state.get() == HeadState::Warm {
                    tp.rearm(i);
                }
            }
        }
    }
}

/// State close: the summary of `RUSTYNAIL_JIT_VERBOSE=1`.
pub(crate) fn report(d: &JitData) {
    if d.tr.cfg.mode == TrMode::Off || d.tr.cfg.verbose == 0 {
        return;
    }
    let mut entered = d.tr.stats.entered;
    for &k in &d.live {
        if let Some(tp) = tr_of(d, k) {
            entered += tp
                .heads()
                .iter()
                .map(|h| u64::from(h.entry_ok.get()))
                .sum::<u64>();
        }
    }
    eprint!(
        "{}",
        print::stats_report(&d.tr.stats, entered, &d.tr.cfg.warnings)
    );
    // PI: exits taken by the traces still live at close (flushed traces take their counters with them)
    let (mut exits, mut traces) = (d.tr.stats.exits, 0u32);
    for sl in &d.tr.slots {
        if let Body::Trace(t) = &sl.body {
            traces += 1;
            exits += trace_exits(t);
            exit_dump(t, "live");
        }
    }
    eprintln!("[trace] exits {exits} (live traces {traces})");
}

/// What the `loopany` guard flushes when it fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaPlan {
    /// The whole family (root included) as `Ineffective`.
    Family,
    /// Only the side traces that unrolled a blocked loop (`laprune`): the root unrolled none.
    Sides,
}

/// C36 (`domtop`): is a root with `e` entries since its trace was installed, `top_any` of which left through its busiest pre-roll site into a side trace, to be
/// re-recorded (once per head: `done`)?
pub fn dom_due(domtop: u32, domn: u32, top_any: u64, e: u64, done: bool) -> bool {
    domtop != 0
        && top_any * 100 >= e * u64::from(domtop)
        && e >= u64::from(domn)
        && (!done || mutants::active("dom_repeats"))
        && !mutants::active("dom_never")
}

/// C36 (`second` bit 0): does an abort that just blacklisted its head (`black`) get the head's second chance? Only a changeable cause of the kind `LeaveFrame`: the
/// recording started at a back-edge from which the loop leaves its function before it closes (a phase effect of the counter trigger; awfy/list at `wgate=45000`). Other
/// changeable causes (`IntMiss`, `LeaveLoop`, `StartRefused`) are data or property of the head and recorded worse traces than tier 1 when retried (regex_nfa +28 %).
pub fn second_retry_due(second: u32, black: bool, unchangeable: bool, reason: &str) -> bool {
    black
        && !unchangeable
        && reason == "LeaveFrame"
        && second & 1 != 0
        && !mutants::active("second_retry_never")
}

/// C39 (`innerretry`): does an abort `reason` that just blacklisted its head (`black`) start retry round `done + 1` of at most `max`?
pub fn inner_retry_due(max: u32, black: bool, reason: &str, done: u8) -> bool {
    black
        && reason == "InnerLoop"
        && (u32::from(done) < max || mutants::active("inner_retry_unbounded"))
        && !mutants::active("inner_retry_never")
}

/// C36 (`second` bit 1): is an identical re-record of a head (the compile equals the trace flushed last) installed once instead of blacklisting the head
/// (`kept_before`: it was already)?
pub fn ident_keep_due(second: u32, kept_before: bool) -> bool {
    second & 2 != 0
        && !mutants::active("second_identical_never")
        && (!kept_before || mutants::active("second_identical_repeats"))
}

/// `loopany` guard policy (pure): the family root that unrolled a callee loop itself is the exit-storm trace (flush the family); a root that
/// unrolled nothing is good, only its unrolling side traces are the storm.
pub fn la_plan(laprune: u32, root_unrolled: bool) -> LaPlan {
    if laprune != 0 && !root_unrolled && !mutants::active("laprune_flushes_root") {
        LaPlan::Sides
    } else {
        LaPlan::Family
    }
}

/// `loopany` guard policy (pure): is a trace flushed by the pruning plan (a side trace of the firing family that unrolled a loop)?
pub fn la_is_victim(same_family: bool, is_side: bool, has_la: bool) -> bool {
    same_family && is_side && has_la
}

/// `loopany` guard: records the callee loops trace `id` unrolled.
pub fn set_la_sites(st: &mut State, id: TrId, la: Vec<(usize, usize)>) {
    let d = d_of(st);
    let nonempty = !la.is_empty();
    let fam = match d.tr.slot_mut(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => {
            t.la = la;
            Some(t.family)
        }
        _ => None,
    };
    if nonempty {
        // C45: the family is no longer clean
        if let Some(Slot {
            body: Body::Trace(f),
            ..
        }) = fam.and_then(|f| d.tr.slot_mut(f))
        {
            f.la_any = true;
        }
        vlog(d, || {
            format!("loopany: trace {id:?} unrolled callee loops, family {fam:?} is not clean for rearmside")
        });
    }
}

/// `loopany` guard: the callee loops unrolled by the traces of the family of `id`.
fn family_la(d: &JitData, id: TrId) -> Vec<(usize, usize)> {
    let Some(Slot {
        body: Body::Trace(t),
        ..
    }) = d.tr.slot(id)
    else {
        return Vec::new();
    };
    let fam = t.family;
    let mut v: Vec<(usize, usize)> = Vec::new();
    for sl in &d.tr.slots {
        if let Body::Trace(x) = &sl.body {
            if x.family == fam {
                for s in &x.la {
                    if !v.contains(s) {
                        v.push(*s);
                    }
                }
            }
        }
    }
    v
}

/// BUNDLE-O O0: attaches the guard classes of the snapshots to the installed trace (diagnostic dump only).
pub fn set_gkinds(st: &mut State, id: TrId, gk: Vec<u16>, dbl: &[u32]) {
    if let Some(Slot {
        body: Body::Trace(t),
        ..
    }) = d_of(st).tr.slot_mut(id)
    {
        t.dbl = vec![false; gk.len()];
        for &s in dbl {
            if let Some(x) = t.dbl.get_mut(s as usize) {
                *x = true;
            }
        }
        t.gk = gk;
    }
}

/// P3 (`ncallee`): the entry segment of trace `id` holds a callee identity guard (`EQREF` on an empty-map snapshot).
pub fn set_entry_callee(st: &mut State, id: TrId, v: bool) {
    if let Some(Slot {
        body: Body::Trace(t),
        ..
    }) = d_of(st).tr.slot_mut(id)
    {
        t.entry_callee = v;
    }
}

/// BUNDLE-L B: the trace `id` holds hoisted bounds guards.
pub fn set_hoisted(st: &mut State, id: TrId, v: bool) {
    if let Some(Slot {
        body: Body::Trace(t),
        ..
    }) = d_of(st).tr.slot_mut(id)
    {
        t.hoisted = v;
    }
}

/// BUNDLE-L B: is the head `pc` of `pr` to be recorded without hoisted bounds guards?
pub fn head_nohoist(st: &mut State, pr: ProtoRef, pc: u32) -> bool {
    let d = d_of(st);
    tr_of(d, pr.0)
        .and_then(|tp| tp.head_index(pc).map(|i| tp.head(i).nohoist.get()))
        .unwrap_or(false)
}

/// P3 (`ncallee`): is the head `pc` of `pr` to be recorded with its callee guards out of the entry segment?
pub fn head_ncallee(st: &mut State, pr: ProtoRef, pc: u32) -> bool {
    if mutants::active("ncallee_never") {
        return false;
    }
    let d = d_of(st);
    d.tr.cfg.limits.ncallee != 0
        && tr_of(d, pr.0)
            .and_then(|tp| tp.head_index(pc).map(|i| tp.head(i).ncallee.get()))
            .unwrap_or(false)
}

/// P3 (`ncallee`): is a persistent entry failure of a root to set the head's flag? Pure rule: the knob is on, the flushed trace had a callee guard in its entry segment.
pub fn ncallee_due(ncallee: u32, entry_callee: bool) -> bool {
    ncallee != 0
        && (entry_callee || mutants::active("ncallee_without_entry_guard"))
        && !mutants::active("ncallee_never")
}

/// P3 (`chainre`): re-orders of one exit's child subtree after which the exit keeps its chain.
pub const CHAIN_MAX_EXIT: u8 = 3;
/// P3 (`chainre`): re-orders of one family after which no chain of it is re-ordered again.
pub const CHAIN_MAX_FAMILY: u32 = 12;
/// P3 (`chainre`): a re-order must save at least this many link visits per entry (percent of one link).
pub const CHAIN_GAIN_PCT: u64 = 100;
/// P3 (`chainre`): longest chain walked.
const CHAIN_WALK: usize = 64;

/// P3 (`chainre`): the pure decision. `v[0]` is the number of entries of the checked side trace (its link `0`), `v[i + 1]` the number of calls that left link `i` through its
/// callee-guard exit (= the entries of link `i + 1`): the chain has `m = v.len() - 1` links and `v[m]` calls leave the last one. The calls that took link `i` are
/// `v[i] - v[i + 1]`. `done`: re-orders of this link's attachment exit so far, `fam`: of the family. True when
///   * `v[0] >= chainn` (entries),
///   * link 0 is a pass-through: `v[1] * 100 >= v[0] * chainre` (at most `100 - chainre` percent of its entries take it),
///   * the chain has at least two links and the order of its links is worse than hotness order by at least [`CHAIN_GAIN_PCT`] percent of a link visit per entry
///     (the thrash bound: a uniform chain, which any order serves equally, never fires),
///   * the exit and the family are within their bounds.
pub fn chain_due(chainre: u32, chainn: u32, v: &[u64], done: u8, fam: u32) -> bool {
    if chainre == 0 || mutants::active("chain_never") || v.len() < 3 {
        return false;
    }
    if (done >= CHAIN_MAX_EXIT || fam >= CHAIN_MAX_FAMILY) && !mutants::active("chain_unbounded") {
        return false;
    }
    if v[0] < u64::from(chainn) {
        return false;
    }
    if v[1] * 100 < v[0] * u64::from(chainre) && !mutants::active("chain_pass_ignored") {
        return false;
    }
    let m = v.len() - 1;
    let mut shares: Vec<u64> = (0..m).map(|i| v[i].saturating_sub(v[i + 1])).collect();
    let cur: u64 = shares
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u64 + 1) * s)
        .sum();
    shares.sort_unstable_by(|a, b| b.cmp(a));
    let ideal: u64 = shares
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u64 + 1) * s)
        .sum();
    let gain = cur - ideal;
    gain * 100 >= v[0] * CHAIN_GAIN_PCT || mutants::active("chain_gain_ignored")
}

/// BUNDLE-G (`chaindepth`): the exit counters of the chain above exit `k` of trace `id`, from the top: `c[0]` is the counter of the exit that hosts the first link (the
/// calls that reached the chain), `c[i]` that of the callee-guard exit of link `i` (the calls that left it), the last is exit `k` of `id` itself, so `c.len() - 1` is
/// the number of links. Empty when exit `k` is not a callee-guard exit (nothing to cap) or `id` is not part of a chain. Counters only: no clock.
pub fn chain_counts(d: &JitData, id: TrId, k: usize) -> Vec<u64> {
    let is_cg = |t: &Trace, e: usize| t.gk.get(e).is_some_and(|g| g & 1 != 0);
    let mut c = Vec::new();
    match d.tr.slot(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) if is_cg(t, k) => c.push(norm_count(t, k, t.exits[k].get())),
        _ => return c,
    }
    let mut cur = id;
    for _ in 0..512 {
        let Some(Slot {
            body: Body::Trace(t),
            ..
        }) = d.tr.slot(cur)
        else {
            break;
        };
        let Some(sl) = t.side.filter(|sl| !sl.prelude) else {
            break;
        };
        let Some(Slot {
            body: Body::Trace(p),
            ..
        }) = d.tr.slot(sl.parent)
        else {
            break;
        };
        let e = sl.exit as usize;
        if !is_cg(p, e) {
            break;
        }
        c.push(norm_count(p, e, p.exits[e].get()));
        cur = sl.parent;
    }
    if c.len() < 2 {
        return Vec::new();
    }
    c.reverse();
    c
}

/// BUNDLE-G (`chaindepth`): the calls a chain of at least `chaindepth` links must serve (percent of the calls that reach its first link) for the chain to be extended.
pub const CHAIN_COVER_PCT: u64 = 30;

/// BUNDLE-G (`chaindepth`): the pure rule over [`chain_counts`] (`hot`: exits a side trace waits for, i.e. what the counter of a link's parent exit held when the link was
/// started). A call that reached link `i` was matched there or left it, so the share of the calls that reach the chain and leave its last link is the product over the
/// links of `c[i] / (c[i-1] - hot)` (the calls that entered link `i` since it exists). True (no further link) when the chain has `cd` links and that share exceeds
/// `100 - CHAIN_COVER_PCT` percent: a wide, flat dispatch, where every further link makes all the unmatched calls pay one more pass-through for a share of about
/// `1 / (callees left)`.
pub fn chain_refuse_rule(cd: u32, c: &[u64], hot: u64) -> bool {
    if cd == 0
        || c.len() < 2
        || ((c.len() - 1) as u32) < cd
        || mutants::active("chaindepth_ignored")
    {
        return false;
    }
    if mutants::active("chaindepth_cover_ignored") {
        return true;
    }
    // fixed point, 1.0 = 1 << 20
    let one = 1u128 << 20;
    let mut left = one;
    for i in 1..c.len() {
        let entered = c[i - 1].saturating_sub(hot).max(1);
        let q = (u128::from(c[i]) * one / u128::from(entered)).min(one);
        left = left * q / one;
    }
    left * 100 > one * (100 - CHAIN_COVER_PCT) as u128
}

/// BUNDLE-G (`chaindepth`): [`chain_refuse_rule`] of the hot callee-guard exit `k` of trace `id`.
pub fn chain_refuse(d: &JitData, id: TrId, k: usize, cd: u32) -> bool {
    let c = chain_counts(d, id, k);
    chain_refuse_rule(cd, &c, u64::from(d.tr.cfg.hot_exit))
}

/// P3 (`chainre`): the callee-guard exit of trace `t` that carries the most exits: `(exit, count)` (the counter is normalised to one count per exit).
fn chain_exit(t: &Trace) -> Option<(usize, u64)> {
    t.exits
        .iter()
        .enumerate()
        .filter(|(k, _)| t.gk.get(*k).is_some_and(|g| g & 1 != 0))
        .map(|(k, c)| (k, norm_count(t, k, c.get())))
        .filter(|&(_, n)| n > 0)
        .max_by_key(|&(_, n)| n)
}

/// P3: all exits `t` took, including those counted before a counter was re-based ([`chain_check`]), in counter units.
fn trace_exits(t: &Trace) -> u64 {
    t.exits
        .iter()
        .enumerate()
        .map(|(k, c)| {
            u64::from(exit_count(c.get()))
                + t.chain_off.get(k).copied().unwrap_or(0)
                    * if t.dbl.get(k).copied().unwrap_or(false) {
                        2
                    } else {
                        1
                    }
        })
        .sum()
}

/// P3: the exit count of exit `k` of `t` as the number of exits (the counter of a frame-cell `mat_stub` exit counts twice per exit; `chain_off` holds what was
/// counted before the counter was re-based).
fn norm_count(t: &Trace, k: usize, c: u32) -> u64 {
    let n = u64::from(exit_count(c));
    t.chain_off.get(k).copied().unwrap_or(0)
        + if t.dbl.get(k).copied().unwrap_or(false) {
            n / 2
        } else {
            n
        }
}

/// P3 (`chainre`): exit `exitno` of trace `id` (a callee-guard exit of a side trace, which has a side trace attached) took another [`exit::CHAIN_EXITS`] exits. A megamorphic
/// call site is a chain of side traces ordered by recording time; a link that carries few calls but sits in front of the hot ones costs a pass-through (an exit stub and a
/// cell jump) per call. When the chain from this trace down passes [`chain_due`] the trace is flushed with everything below it, once per attachment exit (bounded by
/// [`CHAIN_MAX_EXIT`] / [`CHAIN_MAX_FAMILY`]): its parent's exit records the callee that is called now first, then the next one, so that the chain follows the current hotness.
/// Soundness: the flush of a side trace with its subtree is the TR8 case (the parent's cell goes back to its landing address, the counter restarts, the exit is `Free`; no code
/// address of the flushed traces is embedded anywhere: links go through cells), executed through [`request_flush`], so that inside the exit handler the blocks are parked and
/// retired at the next safe point (TR18) and the stubs of the flushed trace land in tier 1; the callee prototypes drop their TR32 edges with the traces. Returns whether `id` was flushed.
pub fn chain_check(st: &mut State, id: TrId, exitno: usize) -> bool {
    let d = d_of(st);
    let (chainre, chainn) = (d.tr.cfg.limits.chainre, d.tr.cfg.limits.chainn);
    if chainre == 0 {
        return false;
    }
    let Some(Slot {
        body: Body::Trace(t),
        ..
    }) = d.tr.slot(id)
    else {
        return false;
    };
    let Some(sl) = t.side.filter(|sl| !sl.prelude) else {
        return false;
    };
    if t.gk.get(exitno).is_none_or(|g| g & 1 == 0)
        || !matches!(t.exit_st.get(exitno), Some(SideSt::Has(_)))
    {
        return false;
    }
    let Some(Slot {
        body: Body::Trace(p),
        ..
    }) = d.tr.slot(sl.parent)
    else {
        return false;
    };
    let k = sl.exit as usize;
    let done = p.chain_done.get(k).copied().unwrap_or(u8::MAX);
    let mut v = vec![
        norm_count(p, k, p.exits[k].get()),
        norm_count(t, exitno, t.exits[exitno].get()),
    ];
    let mut cur = match t.exit_st[exitno] {
        SideSt::Has(c) => c,
        _ => return false,
    };
    for _ in 0..CHAIN_WALK {
        let Some(Slot {
            body: Body::Trace(ct),
            ..
        }) = d.tr.slot(cur)
        else {
            break;
        };
        let Some((k2, n)) = chain_exit(ct) else {
            v.push(0);
            break;
        };
        v.push(n);
        match ct.exit_st.get(k2) {
            Some(SideSt::Has(nx)) => cur = *nx,
            _ => break,
        }
    }
    let family = t.family;
    let parent = sl.parent;
    let fam_n = match d.tr.slot(family) {
        Some(Slot {
            body: Body::Trace(f),
            ..
        }) => f.chain_n,
        _ => 0,
    };
    d.tr.stats.chain_checks += 1;
    vlog(d, || {
        format!("chain check: side trace slot {} (parent slot {} exit {k}) counts {v:?} done {done} family {fam_n}", id.slot, parent.slot)
    });
    if !chain_due(chainre, chainn, &v, done, fam_n) {
        // the counter of the exit is re-based, so that the stub diverts again after another CHAIN_EXITS exits (the inline test is an equality: no mask on the hot path)
        if !mutants::active("chain_no_rebase") {
            let hot = d.tr.cfg.hot_exit;
            if let Some(Slot {
                body: Body::Trace(tm),
                ..
            }) = d.tr.slot_mut(id)
            {
                let c = tm.exits[exitno].get();
                let n = exit_count(c);
                if n > hot {
                    let dbl = tm.dbl.get(exitno).copied().unwrap_or(false);
                    tm.chain_off[exitno] += u64::from(n - hot) / if dbl { 2 } else { 1 };
                    tm.exits[exitno].set(hot | (c & EXIT_BLACK_BIT));
                }
            }
        }
        return false;
    }
    vlog(d, || {
        format!("chain re-order: side trace slot {} (parent slot {} exit {k}) counts {v:?}: flushed with its subtree (re-order {} of this exit)", id.slot, parent.slot, done + 1)
    });
    // the side attempts of the flushed links inside callee frames are given back to the `loopany` guard of the family: the re-recorded chain makes them again
    // (they would block the exits of the new chain after a few links otherwise; the bound of the re-order itself is `CHAIN_MAX_*`)
    let mut n_sub = 0u32;
    let mut stack = vec![id];
    while let Some(x) = stack.pop() {
        if let Some(Slot {
            body: Body::Trace(xt),
            ..
        }) = d.tr.slot(x)
        {
            n_sub += 1;
            stack.extend(xt.kids.iter().copied());
        }
    }
    if let Some(Slot {
        body: Body::Trace(pt),
        ..
    }) = d.tr.slot_mut(parent)
    {
        pt.chain_done[k] = pt.chain_done[k].saturating_add(1);
    }
    if let Some(Slot {
        body: Body::Trace(f),
        ..
    }) = d.tr.slot_mut(family)
    {
        f.chain_n += 1;
        f.la_sides = f.la_sides.saturating_sub(n_sub);
    }
    d.tr.stats.chain_flushes += 1;
    request_flush(st, id, FlushWhy::Chain);
    true
}

/// BUNDLE-O O0: is the per-exit dump (`RUSTYNAIL_JIT_EXITDUMP=1`) on?
pub fn exit_dump_on() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("RUSTYNAIL_JIT_EXITDUMP").is_ok_and(|v| v == "1"))
}

/// BUNDLE-O O0: one line per exit of `t` that was taken (`RUSTYNAIL_JIT_EXITDUMP=1`).
fn exit_dump(t: &Trace, tag: &str) {
    if !exit_dump_on() {
        return;
    }
    let Some(m) = &t.xmeta else { return };
    for (k, c) in t.exits.iter().enumerate() {
        let n = exit_count(c.get());
        if n == 0 || k >= m.exits.len() {
            continue;
        }
        let sn = &m.snaps[m.exits[k].snap as usize];
        let code = sn.frames.last().map_or(sn.code_or(m.root_code), |f| f.code);
        // SAFETY: `code` is the code vector of a prototype the trace depends on (live while the trace is) and `pc` lies in it.
        let w = unsafe { *code.add(sn.pc as usize) };
        let op = rn_core::instruction::Instruction(w).opcode();
        let gk = t.gk.get(m.exits[k].snap as usize).copied().unwrap_or(0);
        eprintln!(
            "[exitdump] {tag} trace {} side {} head {} exit {k} n {n} pc {} op {:?} frames {} gk {gk} st {:?} fails {} last {:?} black {}",
            t.no,
            t.side.is_some(),
            t.head,
            sn.pc,
            op,
            sn.frames.len(),
            t.exit_st.get(k),
            t.fails.get(k).copied().unwrap_or(0),
            t.last.get(k).copied().flatten(),
            c.get() & EXIT_BLACK_BIT != 0
        );
    }
}

/// `jit_tr_hot`'s decision (3.1), after the descriptor lookup.
fn hot(st: &mut State, ci: Ci, pr: ProtoRef, pc: u32) -> bool {
    safe_point(st);
    let d = d_of(st);
    let Some(tp) = tr_of(d, pr.0) else {
        return false;
    };
    let Some(i) = tp.head_index(pc) else {
        return false;
    };
    let h = tp.head(i);
    if d.tr.cfg.mode == TrMode::Off || !d.enabled {
        h.arm(i64::from(i32::MAX / 2));
        return false;
    }
    // A recording whose recorder is gone (root frame unwound, coroutine abandoned, hook disarmed) or that lost a
    // dependency never reports back: drop it here so the state can record again (contract: docs/JIT_TRACE.md 4.2).
    if let Some(aid) = d.tr.active {
        if rec_aborted_flag(d, aid) || !jit_hooks::rec_active_for(st) {
            finish_abort(st, aid, "Abandoned", AbortKind::Drop);
        }
    }
    let d = d_of(st);
    if h.state.get() != HeadState::Warm || d.tr.active.is_some() || jit_hooks::rec_active() {
        tp.rearm(i);
        return false;
    }
    if d.tr.global_pen > 0 {
        d.tr.global_pen -= i64::from(tp.hot);
        tp.rearm(i);
        return false;
    }
    let len = st
        .ci_lclosure(ci)
        .map_or(8, |cl| loop_len(cl.proto().code(), pc as usize));
    let d = d_of(st);
    if !d.tr.warm_elapsed(len) {
        // P-G: the state has run loops for too short a time for a trace to pay off yet. BUNDLE-1: with the work gate the head fires
        // again as soon as the budget is used up (the head that was due first stays first: a gate that re-armed every head to the
        // full threshold changed the order in which the loops of a nest formed their traces, mandelbrot: 16 M entries instead of 1 M)
        let wg = u64::from(d.tr.cfg.limits.wgate);
        if wg != 0 && !mutants::active("wgate_full_rearm") {
            let left =
                (wg.saturating_sub(d.tr.warm_n) / len.max(1) + 1).min(i64::from(tp.hot) as u64);
            d.tr.warm_edges = left;
            h.arm(left as i64);
        } else {
            tp.rearm(i);
        }
        return false;
    }
    let Some(start) = d.tr.starter.or_else(|| DEFAULT_START.get().copied()) else {
        // no recorder yet (prototype before P3a): stay warm, look again much later
        h.arm(i64::from(i32::MAX / 2));
        return false;
    };
    let Ok(id) = begin_recording(st, pr, pc) else {
        return false;
    };
    if start(st, ci, id, pc) {
        true
    } else {
        abort_transient(st, id, "StartRefused");
        false
    }
}

/// `jit_tr_hot(thread, ci, head_pc, 0) -> 0 | 1` (3.1): called by the counting stub when the head counter went
/// negative, with the frame exact and before the head's block count. 1: a recording was armed (`ci.trap = 1` by the
/// recorder's start); the stub leaves to tier-1's `exit_at(H)` so the interpreter runs `H` in the hooked copy. 0:
/// continue at `pcl[H]`.
///
/// # Safety
/// Called only from compiled code with the running thread and its current Lua frame.
pub unsafe extern "C" fn jit_tr_hot(th: *mut Thread, ci: *mut CallInfo, a: u32, _b: u32) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`, `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (jit_hooks::state_from_thread(th.cast()), Ci::from_raw(ci)) };
    match ci_proto_ref(&mut st, ci) {
        Some(pr) => u32::from(hot(&mut st, ci, pr, a)),
        None => 0,
    }
}

/// `jit_tr_entry_fail(thread, ci, head_pc, 0)`: the persistent-entry-failure threshold of a trace was reached
/// (3.3): queues the flush (the failure path runs inside the trace's code block, TR18) and returns 0.
///
/// # Safety
/// Called only from compiled code with the running thread and its current Lua frame.
pub unsafe extern "C" fn jit_tr_entry_fail(
    th: *mut Thread,
    ci: *mut CallInfo,
    a: u32,
    _b: u32,
) -> u32 {
    // SAFETY: as in `jit_tr_hot`.
    let (mut st, ci) = unsafe { (jit_hooks::state_from_thread(th.cast()), Ci::from_raw(ci)) };
    if let Some(pr) = ci_proto_ref(&mut st, ci) {
        let d = d_of(&mut st);
        d.tr.in_handler += 1; // a trace's failure path is running: defer the flush to the next safe point
        if !ci_spare_repair(&mut st, pr, a) {
            entry_fail_hot(&mut st, pr, a);
        }
        let d = d_of(&mut st);
        d.tr.in_handler -= 1;
    }
    0
}

/// C36 (`cispare`): the entry-failure threshold of head `pc` was reached; when the installed trace needs spare `CallInfo` nodes (`TR6b`, `meta.depth`) and the
/// running frame lacks them, they are allocated, the head's failure account restarts and no flush happens. Returns whether it repaired.
/// (Tier 1 with inlined built-ins (`t1inline`) never makes the call that would have extended the chain, so a leaf called at the deepest call depth reached so far
/// fails every entry of a trace with an intrinsic call: a flush re-records the same code, which is then refused as `Identical` and the head is blacklisted.)
fn ci_spare_repair(st: &mut State, pr: ProtoRef, pc: u32) -> bool {
    let d = d_of(st);
    if d.tr.cfg.limits.cispare == 0 || mutants::active("cispare_never") {
        return false;
    }
    let Some(tp) = tr_of(d, pr.0) else {
        return false;
    };
    let Some(i) = tp.head_index(pc) else {
        return false;
    };
    if !tp.entry_fail_hot(i) {
        return false;
    }
    let depth = match tp.head(i).trace.get().and_then(|id| match d.tr.slot(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => t.xmeta.as_ref().map(|m| m.depth),
        _ => None,
    }) {
        Some(n) if n > 0 => usize::from(n),
        _ => return false,
    };
    match st.ensure_spare_ci(depth) {
        Some(added) if added > 0 => {
            let d = d_of(st);
            d.tr.stats.ci_repaired += 1;
            if let Some(tp) = tr_of(d, pr.0).filter(|_| !mutants::active("cispare_flush_anyway")) {
                let h = tp.head(i);
                h.entry_fail.set(0);
                h.entry_ok.set(0);
                h.pending.set(0);
            }
            vlog(d_of(st), || {
                format!("cispare: head pc {pc}: {added} spare CallInfo node(s) allocated (trace needs {depth}), entry-failure account restarted")
            });
            // (mutant `cispare_flush_anyway`: the repair is made, the account stays and the flush follows)
            !mutants::active("cispare_flush_anyway")
        }
        _ => false,
    }
}

/// `jit_tr_move(thread, ci, 0, 0)`: `+tracemove` (8): moves the stack right before a trace entry.
///
/// # Safety
/// Called only from compiled code with the running thread and its current Lua frame.
pub unsafe extern "C" fn jit_tr_move(th: *mut Thread, _ci: *mut CallInfo, _a: u32, _b: u32) -> u32 {
    // SAFETY: as in `jit_tr_hot`.
    let mut st = unsafe { jit_hooks::state_from_thread(th.cast()) };
    d_of(&mut st).tr.stats.moves += 1;
    jit_hooks::stress_move_stack(&mut st);
    0
}

// ---- tier-1 lowering hook ---------------------------------------------------------------------------------

/// What the lowering hook decided for one head.
pub(crate) struct HeadLabels {
    pc: u32,
    d: crate::ir::L,
    c: crate::ir::L,
    m: Option<crate::ir::L>,
    /// F5: J4's promote-counter stub of the head (`None`: the nest is not warm in this version).
    w: Option<crate::ir::L>,
}

/// State of one run of the dispatch hook: the table it creates (or borrows, for a recompile of a prototype that
/// already has one) and the labels to resolve once the code is placed.
pub(crate) struct Dispatch<'t> {
    ext: Option<&'t TrProto>,
    owned: Option<Box<TrProto>>,
    labels: Vec<HeadLabels>,
    cfg: DispatchCfg,
    /// F5: J4's nests of the prototype (attached to the table by the first compile).
    nests: Vec<(u32, u32)>,
    region_first: bool,
    /// F5 `+allblack`: every head starts `Black`.
    all_black: bool,
    /// F5: the nests (pc ranges) whose heads this version has no dispatch for: the region of a promoted nest owns the
    /// loop, its heads are `Black` for good and need no `D_H`.
    skip: Vec<(u32, u32)>,
}

impl<'t> Dispatch<'t> {
    /// A first compile (`ext = None`: the table is created by the hook) or a recompile.
    pub(crate) fn new(cfg: DispatchCfg, ext: Option<&'t TrProto>) -> Dispatch<'t> {
        Dispatch {
            ext,
            owned: None,
            labels: Vec::new(),
            cfg,
            nests: Vec::new(),
            region_first: false,
            all_black: false,
            skip: Vec::new(),
        }
    }

    /// F5: J4's nests of the prototype (`scan_nests`); the table gets them attached by [`Dispatch::finish`].
    pub(crate) fn with_nests(mut self, nests: &[(u32, u32)], region_first: bool) -> Self {
        self.nests = nests.to_vec();
        self.region_first = region_first;
        self
    }

    /// F5 `+allblack`: every head of the new table starts `Black`.
    pub(crate) fn all_black(mut self, on: bool) -> Self {
        self.all_black = on;
        self
    }

    /// F5: no dispatch for the heads inside these (promoted) nests.
    pub(crate) fn skipping(mut self, skip: &[(u32, u32)]) -> Self {
        self.skip = skip.to_vec();
        self
    }

    /// `(pc, D_H label, C_H label)` of every head (tests).
    #[doc(hidden)]
    pub fn head_labels(&self) -> Vec<(u32, crate::ir::L, crate::ir::L)> {
        self.labels.iter().map(|l| (l.pc, l.d, l.c)).collect()
    }

    /// Bytes of the table created by the hook (0 without heads or for a borrowed table).
    pub(crate) fn table_bytes(&self) -> usize {
        self.owned.as_ref().map_or(0, |t| t.bytes())
    }

    fn resolve(&self, tp: &TrProto, base: usize, low: &Lowered, em: &Emitted) {
        let pcmap = crate::t15::runtime::pc_map(low, em);
        tp.set_pc_map(base, &crate::t15::runtime::land_map(low, em));
        let off = |l: crate::ir::L| em.label_off.get(l as usize).copied().flatten();
        for (i, h) in tp.heads().iter().enumerate() {
            let pcl = pcmap
                .get(h.pc as usize)
                .copied()
                .filter(|&o| o != 0)
                .map_or(0, |o| base + o as usize - 1);
            // (a head without labels in this version: the dispatch is gone, the promoted nest's region owns it)
            let hl = self.labels.iter().find(|l| l.pc == h.pc);
            let c = hl.and_then(|l| off(l.c)).map_or(0, |o| base + o);
            let m = hl.and_then(|l| l.m).and_then(off).map_or(0, |o| base + o);
            let w = hl.and_then(|l| l.w).and_then(off).map_or(0, |o| base + o);
            tp.set_version_w(i, c, m, pcl, w);
        }
    }

    /// First compile: resolves the labels against the placed code at `base`, arms the counters and hands the table
    /// over (`None` when the function has no candidate head).
    pub(crate) fn finish(
        mut self,
        base: usize,
        low: &Lowered,
        em: &Emitted,
    ) -> Option<Box<TrProto>> {
        let tp = self.owned.take()?;
        if !self.nests.is_empty() {
            tp.attach_nests(&self.nests, self.region_first);
        }
        if self.all_black {
            for i in 0..tp.heads().len() {
                tp.blacklist(i);
            }
        }
        self.resolve(&tp, base, low, em);
        tp.sync_all_nests();
        for i in 0..tp.heads().len() {
            tp.rearm(i);
        }
        Some(tp)
    }

    /// Recompile (TR19b(c)): the version-install path rewrites every non-trace cell to the new version's labels
    /// in the same step that installs the version. `Traced` cells keep pointing at their traces.
    pub(crate) fn install_version(&self, base: usize, low: &Lowered, em: &Emitted) {
        if let Some(tp) = self.ext {
            self.resolve(tp, base, low, em);
        }
    }
}

/// R2: most continuation heads (targets of native returns) of one prototype (`TraceEnv::rec_sites` has this many entries).
pub const MAX_CONT_SITES: usize = 6;

/// Parameters of [`emit_dispatch`].
#[derive(Clone, Copy)]
pub(crate) struct DispatchCfg {
    pub hot: i32,
    pub always: bool,
    pub tracemove: bool,
    pub seed: u32,
    pub line: i32,
    /// P-E2: the prototype may get a function-entry head (not vararg, `fnroot` limit on).
    pub fn_root: bool,
    /// P-M3: native self-recursion: function-entry heads also get continuation heads.
    pub recnat: bool,
    /// BUNDLE-K L1: `TrLimits::lazyci != 0`.
    pub lazy: bool,
    /// M1: a function with loops also gets the function-entry head (a loop-free path through it is what the recording sees).
    pub fn_loops: bool,
    /// R2: continuation heads for any callee load, native `RETURN0` (`TrLimits::recany`).
    pub rec_any: bool,
    /// BUNDLE-J (`centry`): the prototype was first hot when called from C: it gets the function-entry head although it has no `CALL`, as a generic function root.
    pub c_entered: bool,
}

fn mem(base: crate::ir::V, off: i32) -> M {
    M {
        base: Base::Reg(base),
        off,
    }
}

/// Emits, for every trace candidate head, the dispatch jump `D_H`, the counting stub `C_H` and (with `+tracemove`)
/// the stack-move stub, and routes every edge into the head through `D_H` (TR20). Runs as the region hook of
/// `lower_ex`, i.e. after the tier-1 code of every instruction and before the out-of-line code.
pub(crate) fn emit_dispatch(
    cx: &mut Cx<'_>,
    env: &RegionEnv<'_>,
    dis: &mut Dispatch<'_>,
) -> RegionOut {
    let c = dis.cfg;
    let mut pcs: Vec<u32> = env
        .backedges
        .iter()
        .map(|&(_, h)| h)
        .filter(|&h| {
            let h = h as usize;
            env.kinds[h] != Kind::Stub
                && !env.follower[h]
                && !dis
                    .skip
                    .iter()
                    .any(|&(lo, hi)| (lo..=hi).contains(&(h as u32)))
        })
        .collect();
    // P-E2: a loop-free function that calls something gets the entry head (its callees are what a recording can inline)
    let fn_head = c.fn_root
        && (env.backedges.is_empty() || c.fn_loops)
        && env.kinds[0] != Kind::Stub
        && !env.follower[0]
        && (env.ops.iter().any(|o| matches!(o, Some(OpCode::Call)))
            || (c.c_entered && !mutants::active("centry_no_head"))
            || mutants::active("fn_head_calls_ignored"))
        && (!env.ops.iter().any(|o| matches!(o, Some(OpCode::Tbc)))
            || mutants::active("fn_head_tbc_ignored"));
    let mut conts: Vec<u32> = Vec::new();
    if fn_head {
        pcs.push(0);
        // P-M3: continuation heads: the instruction after a `CALL` whose callee was just read from an upvalue (a recursive
        // local function), where a native `RETURN` of a self-call continues (the "up" traces). R2 (`rec_any`): also a callee
        // read by `SELF` (method), `GETTABUP` (global function) or `GETFIELD`, at most `MAX_CONT_SITES` sites.
        if c.recnat {
            for (pc, i) in env.ins.iter().enumerate() {
                if !matches!(env.ops[pc], Some(OpCode::Call)) || pc + 1 >= env.ins.len() {
                    continue;
                }
                let a = i.a();
                let uv = if c.rec_any {
                    // R2: the callee load is the nearest instruction before the `CALL` that writes register `A`, up to 12 back (the
                    // arguments of a method call can take that many instructions)
                    (1..=12usize)
                        .filter(|k| *k <= pc)
                        .find(|k| env.ins[pc - k].a() == a && env.ops[pc - k].is_some())
                        .is_some_and(|k| {
                            matches!(
                                env.ops[pc - k],
                                Some(
                                    OpCode::Getupval
                                        | OpCode::SelfOp
                                        | OpCode::Gettabup
                                        | OpCode::Getfield
                                )
                            )
                        })
                } else {
                    (1..=3usize).filter(|k| *k <= pc).any(|k| {
                        matches!(env.ops[pc - k], Some(OpCode::Getupval))
                            && env.ins[pc - k].a() == a
                    })
                };
                let p = pc + 1;
                if uv
                    && env.kinds[p] != Kind::Stub
                    && !env.follower[p]
                    && conts.len() < MAX_CONT_SITES
                {
                    pcs.push(p as u32);
                    conts.push(p as u32);
                }
            }
        }
    }
    pcs.sort_unstable();
    pcs.dedup();
    let mut out = RegionOut::default();
    // (a function without loops still gets a table: it is the home of the dependency edges of its callers' traces)
    if dis.ext.is_none() {
        let tp = TrProto::new(env.ins.len(), &pcs, c.hot, c.always, c.seed, c.line);
        // BUNDLE-K L1: a loop-free function with the entry head of native recursion is lazy-aware (all of its traces, cells and heads)
        let lazy = c.lazy && fn_head && c.recnat && env.backedges.is_empty() && !c.c_entered;
        tp.lazy.set(lazy);
        for h in tp.heads() {
            h.lazy.set(lazy);
        }
        dis.owned = Some(Box::new(tp));
    }
    let tp: &TrProto = match (dis.ext, dis.owned.as_deref()) {
        (Some(t), _) => t,
        (None, Some(t)) => t,
        _ => return out,
    };
    if fn_head {
        tp.fn_head.set(true);
        *tp.conts.borrow_mut() = conts;
        if c.c_entered && !mutants::active("centry_not_promoted") {
            // (a C-entered function is a generic function root from its first recording: nothing to probe for)
            tp.cent.set(true);
            tp.fnpromo.set(true);
        }
    }
    for &pc in &pcs {
        let Some(i) = tp.head_index(pc) else {
            continue; // (a recompile never has heads the first compile did not)
        };
        let h = tp.head(i);
        let cell = h.cell.as_ptr() as usize as u64;
        let ctr = h.ctr.as_ptr() as usize as u64;
        let entry = h.entry.as_ptr() as usize as u64;
        let (d, cl, fire) = (cx.p.label(), cx.p.label(), cx.p.label());
        let pcl = cx.pcl[pc as usize];
        // D_H: the dispatch jump. Black and Traced heads pay exactly these three ops.
        cx.op(Op::Bind(d));
        cx.op(Op::Mov(7, cell));
        cx.ld(7, W::B8, mem(7, 0));
        cx.op(Op::JmpR(7));
        // C_H: the counting stub; falls into pcl[H], which is downstream of D_H.
        cx.op(Op::Bind(cl));
        cx.op(Op::Mov(0, ctr));
        cx.ld(1, W::B4, mem(0, 0));
        cx.op(Op::Sub(1, Src::Imm(1)));
        cx.st(W::B4, mem(0, 0), 1);
        cx.br(Cc::Lt, 1, Src::Imm(0), fire);
        cx.op(Op::Jmp(pcl));
        cx.op(Op::Bind(fire));
        cx.op(Op::CallH {
            f: jit_tr_hot as *const () as usize as u64,
            a: pc,
            b: 0,
        });
        if let Some(e) = cx.cfg.calls {
            cx.op(Op::ReloadFrame {
                stack_off: e.stack_off,
            });
        }
        cx.br(Cc::Eq, 0, Src::Imm(0), pcl);
        let x = cx.exit_at(pc);
        cx.op(Op::Jmp(x));
        let m = if c.tracemove && cx.cfg.calls.is_some() {
            let m = cx.p.label();
            cx.op(Op::Bind(m));
            cx.op(Op::CallH {
                f: jit_tr_move as *const () as usize as u64,
                a: pc,
                b: 0,
            });
            if let Some(e) = cx.cfg.calls {
                cx.op(Op::ReloadFrame {
                    stack_off: e.stack_off,
                });
            }
            cx.op(Op::Mov(7, entry));
            cx.ld(7, W::B8, mem(7, 0));
            cx.op(Op::JmpR(7));
            Some(m)
        } else {
            None
        };
        out.heads.push((pc, d));
        out.resume_overrides.push((pc, d));
        dis.labels.push(HeadLabels {
            pc,
            d,
            c: cl,
            m,
            w: env.warm_stub.get(&pc).copied(),
        });
    }
    out
}

// ---- test support -----------------------------------------------------------------------------------------

/// Snapshot of a head for tests.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadInfo {
    /// pc.
    pub pc: u32,
    /// State.
    pub state: HeadState,
    /// Counter.
    pub ctr: i32,
    /// Cell.
    pub cell: usize,
    /// `C_H`.
    pub c_addr: usize,
    /// Tracemove stub.
    pub m_addr: usize,
    /// `pcl[H]`.
    pub pcl_addr: usize,
    /// Trace entry.
    pub entry: usize,
    /// `k`.
    pub k: u8,
    /// `kt`.
    pub kt: u8,
    /// `rerec`.
    pub rerec: u8,
    /// Entry counters.
    pub entry_fail: u32,
    /// Entry counters.
    pub entry_ok: u32,
    /// Entry counters.
    pub pending: u32,
    /// Trace or recording.
    pub trace: Option<TrId>,
    /// F5: `W_H` (J4's promote-counter stub) in the current version.
    pub w_addr: usize,
    /// F5: the cell targets `W_H`.
    pub region_wait: bool,
}

/// Snapshot of the heads of `pr`.
#[doc(hidden)]
pub fn head_infos(st: &mut State, pr: ProtoRef) -> Vec<HeadInfo> {
    with_tr(st, pr, |tp| {
        tp.heads()
            .iter()
            .enumerate()
            .map(|(i, h)| HeadInfo {
                pc: h.pc,
                state: h.state.get(),
                ctr: h.ctr.get(),
                cell: h.cell.get(),
                c_addr: h.c_addr.get(),
                m_addr: h.m_addr.get(),
                pcl_addr: h.pcl_addr.get(),
                entry: h.entry.get(),
                k: h.k.get(),
                kt: h.kt.get(),
                rerec: h.rerec.get(),
                entry_fail: h.entry_fail.get(),
                entry_ok: h.entry_ok.get(),
                pending: h.pending.get(),
                trace: h.trace.get(),
                w_addr: tp.ext[i].w_addr.get(),
                region_wait: tp.ext[i].region_wait.get(),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Attaches the exit metadata to the installed trace `id` (right after [`install`]); replaces an earlier one. `None`
/// when `id` is stale.
pub fn attach_exits(st: &mut State, id: TrId, meta: ExitMeta) -> Option<Arc<ExitMeta>> {
    match d_of(st).tr.slot_mut(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => {
            // F2a: an exit whose cell does not start at its tier-1 landing address is chained (a prelude's entry checks
            // continue in the root trace): nothing may replace that
            for (k, home) in t.cells_init.iter().enumerate() {
                // (S1: the cell of an exit with virtual-able frames starts at 0)
                if meta.snaps.get(k).is_some_and(|s| {
                    s.land_addr as usize != *home && !(*home == 0 && s.frame_side_ok())
                }) && k < t.exit_st.len()
                {
                    black_exit(&mut t.exit_st, &t.exits, k);
                }
            }
            if t.no_start_side {
                for (k, sn) in meta.snaps.iter().enumerate() {
                    if sn.slots.is_empty() && sn.frames.is_empty() && k < t.exit_st.len() {
                        black_exit(&mut t.exit_st, &t.exits, k);
                    }
                }
            }
            let m = Arc::new(meta);
            t.xmeta = Some(Arc::clone(&m));
            Some(m)
        }
        _ => None,
    }
}

/// The live trace in `slot` and its exit metadata (the exit handler's lookup by `ExitArea.trace_id`).
pub fn exit_meta(st: &mut State, slot: u32) -> Option<(TrId, Arc<ExitMeta>)> {
    let sl = d_of(st).tr.slots.get(slot as usize)?;
    match &sl.body {
        Body::Trace(t) => Some((TrId { slot, gen: sl.gen }, Arc::clone(t.xmeta.as_ref()?))),
        _ => None,
    }
}

/// [`exit_meta`] without the reference count: a raw pointer to the metadata of the live trace in `slot` (P-L). The exit
/// handler uses it so that an exit costs no atomic increment and decrement (about 40 cycles of the ~127 an exit took).
///
/// Validity: the pointee is owned by the trace body (`Trace::xmeta`). While the handler runs (`in_handler > 0`, set by
/// [`handler_enter`], which the caller must call *before* the first use of the pointer and keep until its last) a flush
/// only parks the body in the graveyard ([`safe_point`] and [`retire`] do nothing while a handler is active), and
/// `attach_exits` runs only when a trace is installed, never inside a handler. So the pointer stays valid until
/// [`handler_leave`].
pub fn exit_meta_ptr(st: &mut State, slot: u32) -> Option<(TrId, *const ExitMeta)> {
    let sl = d_of(st).tr.slots.get(slot as usize)?;
    match &sl.body {
        Body::Trace(t) => Some((TrId { slot, gen: sl.gen }, Arc::as_ptr(t.xmeta.as_ref()?))),
        _ => None,
    }
}

/// What the slot `slot` of the trace table holds (diagnostics of the exit handler): "free", "recording", "trace
/// (side)", "trace (root)" or "trace without metadata".
#[doc(hidden)]
pub fn slot_kind(st: &mut State, slot: u32) -> &'static str {
    match d_of(st).tr.slots.get(slot as usize).map(|s| &s.body) {
        None => "out of range",
        Some(Body::Free) => "free",
        Some(Body::Rec(_)) => "recording",
        Some(Body::Trace(t)) if t.xmeta.is_none() => "trace without metadata",
        Some(Body::Trace(t)) if t.side.is_some() => "trace (side)",
        Some(Body::Trace(_)) => "trace (root)",
    }
}

/// Is `id` a live trace?
#[doc(hidden)]
pub fn trace_live(st: &mut State, id: TrId) -> bool {
    matches!(
        d_of(st).tr.slot(id),
        Some(Slot {
            body: Body::Trace(_),
            ..
        })
    )
}

/// The code block of trace `id` as `(address, size)`.
#[doc(hidden)]
pub fn trace_block(st: &mut State, id: TrId) -> Option<(usize, usize)> {
    match d_of(st).tr.slot(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => t.block.as_ref().map(|b| (b.addr(), b.size())),
        _ => None,
    }
}

/// Exit counters of trace `id`.
#[doc(hidden)]
pub fn trace_exit_counters(st: &mut State, id: TrId) -> Vec<u32> {
    match d_of(st).tr.slot(id) {
        Some(Slot {
            body: Body::Trace(t),
            ..
        }) => t.exits.iter().map(|c| exit_count(c.get())).collect(),
        _ => Vec::new(),
    }
}

/// The `(address, size)` of the blocks parked in the graveyard.
#[doc(hidden)]
pub fn grave_blocks(st: &mut State) -> Vec<(usize, usize)> {
    d_of(st)
        .tr
        .grave
        .iter()
        .filter_map(|t| t.block.as_ref().map(|b| (b.addr(), b.size())))
        .collect()
}

/// Number of live traces and of traces waiting in the graveyard.
#[doc(hidden)]
pub fn counts(st: &mut State) -> (u32, usize) {
    let d = d_of(st);
    (d.tr.live_traces, d.tr.grave.len())
}

/// The statistics of this state.
pub fn tr_stats(st: &mut State) -> TrStats {
    d_of(st).tr.stats.clone()
}

/// The recording active in this state, if any.
#[doc(hidden)]
pub fn active_recording(st: &mut State) -> Option<TrId> {
    d_of(st).tr.active
}

/// The global penalty still pending (back-edges).
#[doc(hidden)]
pub fn global_penalty(st: &mut State) -> i64 {
    d_of(st).tr.global_pen
}

/// Machine code of a *fake trace* (tests; there is no compile pipeline yet): increments the `u64` at `counter`,
/// optionally calls `handler(thread, ci, a, b)`, then jumps to `land` (`pcl[H]`, i.e. the trace "fails its entry"
/// and tier 1 continues; the computation is unchanged).
#[doc(hidden)]
pub fn fake_trace_code(counter: usize, land: usize, handler: Option<(u64, u32, u32)>) -> Vec<u8> {
    let mut p = Program::default();
    p.push(Op::Mov(0, counter as u64));
    p.push(Op::Ld {
        dst: 1,
        w: W::B8,
        m: mem(0, 0),
    });
    p.push(Op::Add(1, Src::Imm(1)));
    p.push(Op::St {
        w: W::B8,
        m: mem(0, 0),
        src: Src::V(1),
    });
    if let Some((f, a, b)) = handler {
        p.push(Op::CallH { f, a, b });
    }
    p.push(Op::Mov(7, land as u64));
    p.push(Op::JmpR(7));
    x86::emit(&p).expect("fake trace").code
}

/// What [`lower_probe`] found.
#[doc(hidden)]
pub struct Probe {
    /// The micro-ops of the whole function.
    pub ops: Vec<Op>,
    /// `(pc, D_H, C_H)` per head (empty without the hook).
    pub heads: Vec<(u32, crate::ir::L, crate::ir::L)>,
    /// The tier-1 label of every pc.
    pub pc_labels: Vec<Option<crate::ir::L>>,
}

fn caller_closure_proto(st: &mut State) -> Option<rn_core::value::Gc<rn_core::func::Proto>> {
    let ci = st.ci().previous()?;
    Some(rt::proto_of(st, ci)?.proto())
}

/// Lowers the calling Lua function with (`trace = true`) or without the dispatch hook and returns the micro-ops (the
/// "dump check" of the dispatch path, tests).
#[doc(hidden)]
pub fn lower_probe(st: &mut State, trace: bool) -> Option<Probe> {
    let p = caller_closure_proto(st)?;
    let d = d_of(st);
    let env = rt::call_env(st, d);
    let cfg = crate::lower::LowerCfg {
        code_addr: p.code_ptr() as usize,
        interrupt_addr: jit_hooks::interrupt_flag_addr(),
        calls: Some(env),
        consts: p.k(),
        t15_warmup: None,
    };
    if !trace {
        let low = crate::lower::lower(p.code(), cfg)?;
        return Some(Probe {
            ops: low.prog.ops,
            heads: Vec::new(),
            pc_labels: low.pc_labels,
        });
    }
    let mut dis = Dispatch::new(
        DispatchCfg {
            hot: 4,
            always: true,
            tracemove: d.tr.cfg.opts.tracemove,
            seed: 1,
            line: p.linedefined,
            fn_root: false,
            recnat: false,
            lazy: false,
            fn_loops: false,
            rec_any: false,
            c_entered: false,
        },
        None,
    );
    let low = {
        let mut hook = |cx: &mut Cx<'_>, env: &RegionEnv<'_>| emit_dispatch(cx, env, &mut dis);
        crate::lower::lower_ex(p.code(), cfg, Some(&mut hook), false)?
    };
    Some(Probe {
        ops: low.prog.ops,
        heads: dis.head_labels(),
        pc_labels: low.pc_labels,
    })
}

/// `(bytes the state generated for the calling Lua function, bytes of the plain `lower` + `emit`)`: with traces off
/// they must be equal (the dump identity of 3.1, in-process).
#[doc(hidden)]
pub fn tier1_bytes(st: &mut State) -> Option<(Vec<u8>, Vec<u8>)> {
    let p = caller_closure_proto(st)?;
    let jp = p.jit.get() as usize;
    if jp <= rt::NOJIT {
        return None;
    }
    // SAFETY: a compiled prototype's `jit` is its live descriptor (the caller is running it).
    let jp = unsafe { &*(jp as *const JitProto) };
    let d = d_of(st);
    let env = rt::call_env(st, d);
    let cfg = crate::lower::LowerCfg {
        code_addr: p.code_ptr() as usize,
        interrupt_addr: jit_hooks::interrupt_flag_addr(),
        calls: Some(env),
        consts: p.k(),
        t15_warmup: None,
    };
    let expected = x86::emit(&crate::lower::lower(p.code(), cfg)?.prog)
        .ok()?
        .code;
    // SAFETY: the block is the committed, readable code of the live descriptor.
    let actual = unsafe {
        std::slice::from_raw_parts(
            jp.block.addr() as *const u8,
            expected.len().min(jp.block.size()),
        )
    }
    .to_vec();
    Some((actual, expected))
}

/// BUNDLE-J: is the function-entry head of `pr` a C-entered function root (`centry`)?
pub fn fn_cent(st: &mut State, pr: ProtoRef) -> bool {
    let d = d_of(st);
    tr_of(d, pr.0).is_some_and(|tp| tp.is_cent())
}

/// BUNDLE-C: is the function-entry head of `pr` promoted to a generic function root (`fnpromo`)?
pub fn fn_promoted(st: &mut State, pr: ProtoRef) -> bool {
    let d = d_of(st);
    tr_of(d, pr.0).is_some_and(|tp| tp.fn_promoted())
}
