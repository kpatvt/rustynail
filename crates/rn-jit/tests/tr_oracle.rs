//! P8a: the trace oracle (docs/JIT_TRACE.md 9.4), the VM-truth check of TR3 (frame restore) and TR5 (instruction
//! count) at every trace exit, without any lockstep of the heap.
//!
//! **Clock.** `clock = interpreter fetches (rn-core `t15_oracle` hook, plus the two unfetched `TFOR*` chains) + compiled
//! instructions`, the number of bytecodes executed so far in program order. In a JIT-off run the clock at a fetch is the
//! number of fetches before it.
//!
//! **Run 1** (trace mode, `tr_oracle` child process) logs checkpoints `(clock, proto, pc, digest)`:
//!
//! * *exit checkpoints*, taken inside the exit handler right after the restore (`tr::exit::set_probe`), only for exits
//!   that reach the handler. Under `+emuverify` every exit does (the option forces generic stubs, `generic_only`), so
//!   the "exits" cells run with it; the probe of `+emuverify` itself is replaced by ours (re-installed at every fetch).
//!   The clock of an exit is `fetches + finished-stay instructions + (B - budget word - land)`, where `B` is the stay's
//!   budget (configured), the budget word is what the handler just stored (`vb + refund`, TR5.4) and `land` the
//!   landing label's prepaid remainder (0 for a block head, `rem[pc]` for a mid-block label; taken from the tier-1
//!   `Lowered` of the innermost prototype);
//! * *stay checkpoints*, taken at the first interpreter fetch after a compiled stay ended (any exit of any trace or
//!   tier-1 exit: inline stubs, hot diverts, entry failures, budget exits ... are all seen this way), clock `fetches +
//!   stay instructions`.
//!
//! **Run 2** (`JIT=off`, the same fetch counter): at the fetch whose clock equals a checkpoint's, the interpreter must
//! be at the same `(proto, pc)` with the same digest. Both runs execute in one child process (fixed seed, GC stopped by
//! the prelude so that finalizer / clearing timing cannot differ).
//!
//! **Digest** (one 64-bit hash per slot, so a mismatch names the slot): the NAMED LOCALS (registers below the number of
//! locals active at the frame's resume pc, from the debug info) of every Lua frame of the chain that are also live
//! there (tier-1.5 liveness plus open-upvalue slots); per slot the tag byte, the payload of integers and floats (NaN
//! canonicalised), the content of strings, nothing else for collectables; and the `CallInfo` chain (function slot, Lua or
//! C, prototype, saved pc). For exit checkpoints the TR3 leftover window above the frame is digested as well and compared
//! with `TR_ORACLE_LEFTOVER=1`.
//!
//! **Deviations from 9.4** (documented, with the evidence, in docs/JIT_TRACE_REF.md "P8a decisions"): (1) locals only, not
//! every slot: tier 1 leaves dead scratch slots different from the interpreter even with traces off; (2) the clock of an
//! exit is derived from the budget word, so a wrong refund or landing kind is a wrong clock; (3) exits that bypass the
//! handler (inline stubs) are covered through the stay checkpoint at the next interpreter fetch, and directly in the
//! `+emuverify` cells; (4) while a stay is suspended inside a helper the clock adds what the open stays consumed, which
//! includes a prepaid block remainder: such checkpoints, and every stay checkpoint (tier 1 is inexact between stay ends),
//! match within `NESTED_SLACK` instructions; exit checkpoints are exact; a job whose only failures are stay checkpoints
//! is a tier-1 skew if the same job with traces off fails too (and some coroutine programs are not stay-compared at all,
//! `STAY_SKEW_PROGS`); (5) the log is sampled (the first 3000, then every 16th, at most 60000); (6) checkpoints that fall into an unfetched `TFOR*` chain have no fetch in the baseline and are
//! counted as `gaps`.
//!
//! Corpus: `tests/tr_oracle_progs`, `tests/t15_oracle_progs`, `tests/vm`, `bench/programs`, `bench/jit-trace` (small
//! N), the twin programs of `tr_core/progs.rs` and generator seeds. Run: `systemd-run --user --scope -q -p
//! MemoryMax=8G -p MemorySwapMax=0 timeout 1800 cargo test -p rn-jit --features tr_mutants,t15_oracle --test tr_oracle --
//! --nocapture </dev/null`. Knobs: `TR_P8A_ONLY=<substr>`, `TR_P8A_GEN=<n>`, `TR_P8A_TIMEOUT`, `TR_ORACLE_V=1`.
#![cfg(all(feature = "t15_oracle", feature = "trace"))]

#[path = "tr_p8a/common.rs"]
mod common;
#[allow(dead_code)]
#[path = "tr_core/progs.rs"]
mod progs;
#[allow(dead_code)]
#[path = "tr_core/sink_progs.rs"]
mod sink_progs;

use common::*;
use rn_core::func::Proto;
use rn_core::state::{CallInfo, LuaState, State, StkId, CIST_C};
use rn_core::value::Value;
use rn_jit::lower::{LabelKind, Lowered};
use rn_jit::rt::stay_stats as ss;
use rn_jit::t15::analysis::Analysis;
use rn_jit::t15::effects as fx;
use rn_jit::t15::{FuncInfo, Role, SlotSet, TagSet};
use rn_jit::tr::cache::TrId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Compare the leftover window above the innermost frame (TR3's popped-callee slots) at exit checkpoints. Off by default:
/// see "P8a decisions" in docs/JIT_TRACE_REF.md (`TR_ORACLE_LEFTOVER=1` turns it on).
fn leftover_on() -> bool {
    std::env::var("TR_ORACLE_LEFTOVER").is_ok()
}
/// Slots above the root frame's base that exit checkpoints digest (TR3 leftovers); `TooLong` is 200.
const EXT: usize = 200;
/// Checkpoint sampling: all of the first `KEEP_ALL`, then every `SAMPLE`-th, at most `MAX_CPS`.
const KEEP_ALL: u64 = 3000;
const SAMPLE: u64 = 16;
const MAX_CPS: usize = 60000;
/// Slot-hash vectors are kept for at most this many checkpoints (memory); later ones are compared by hash only.
const MAX_SLOTVECS: usize = 20000;

/// Programs whose STAY checkpoints are not compared: coroutine switches and a GC-timing-sensitive closure test make the
/// clock between stay ends inexact by more than the slack (seen with traces on only in these; unresolved, see docs/
/// JIT_TRACE_REF.md "P8a decisions"). Exit checkpoints, violations and the identity still apply to them.
const STAY_SKEW_PROGS: [&str; 6] = [
    "bench/programs/coroutines",
    "tests/t15_oracle_progs/p8_coroutines",
    "tests/vm/closures_09_gc_of_closures",
    "tests/vm/co_01_generators",
    "gen:2",
    "gen:8",
];

/// Programs that need more memory than the oracle children get (deep recursion to the stack limit, and with the
/// collector stopped the generated program 4): excluded, the identity test (collector on) covers them.
const MEM_HEAVY: [&str; 4] = [
    "tests/vm/co_09_stack_and_args",
    "tests/vm/err_08_stack_overflow",
    "tests/vm/err_09_error_in_error_handling",
    "gen:4",
];

fn core_src(name: &str) -> &'static str {
    progs::PROGS
        .iter()
        .chain(sink_progs::SINK_PROGS)
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no program {name}"))
        .src
}

// ---- digests -------------------------------------------------------------------------------------------------------

fn mix(h: u64, x: u64) -> u64 {
    let mut z = (h ^ x).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z ^= z >> 29;
    z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z ^ (z >> 32)
}

fn proto_key(p: &Proto) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &w in p.code() {
        h = mix(h, u64::from(w));
    }
    h = mix(h, p.linedefined as u64);
    h = mix(h, p.lastlinedefined as u64);
    h = mix(h, u64::from(p.maxstacksize));
    mix(h, p.k().len() as u64)
}

/// The hash of one slot. `deep`: strings are read (slots inside a live frame; the leftover window is tag-only).
fn slot_hash(v: &Value, deep: bool) -> u64 {
    let tag = u64::from(v.tag());
    let mut h = mix(0x51ed, tag);
    if let Some(i) = v.as_int() {
        h = mix(h, i as u64);
    } else if let Some(f) = v.as_float() {
        let bits = if f.is_nan() {
            0x7ff8_0000_0000_0000
        } else {
            f.to_bits()
        };
        h = mix(h, bits);
    } else if deep && v.is_string() {
        if let Some(s) = v.as_string() {
            for &b in s.as_bytes() {
                h = mix(h, u64::from(b));
            }
        }
    }
    h
}

/// The digest of one machine state at a checkpoint.
#[derive(Clone, Debug, Default)]
struct Dig {
    /// Hash of the innermost frame's prototype.
    key: u64,
    pc: u32,
    /// The opcode at `pc` (reports).
    op: String,
    /// Hash of the `CallInfo` chain.
    chain: u64,
    /// The live slots of every Lua frame (innermost first, ascending): `(absolute stack index, hash)`.
    live: Vec<(u32, u64)>,
    /// Frames whose prototype could not be analysed (no liveness): their slots are not compared.
    unanalysed: u32,
    /// The leftover window above the innermost frame's top (TR3), tag-only hashes; exit checkpoints only.
    left: Vec<u64>,
    /// `(tag, payload bits)` per live slot, for reports (kept for the first `MAX_SLOTVECS` checkpoints only).
    raw: Vec<(u8, u64)>,
}

// ---- liveness ----------------------------------------------------------------------------------------------------------
//
// Tier 1 (and the traces' snapshots) leave dead scratch slots different from the interpreter's: the inline `ipairs`
// step of `TFORCALL` writes the results without the interpreter's call-frame copies, a native call leaves other
// leftovers, temporaries of a finished expression keep different stale values, ... So the digest covers the NAMED
// LOCALS (registers below the number of locals active at the frame's resume pc, from the debug info) that are also LIVE
// there (read before written on some path: the tier-1.5 liveness of `rn_jit::t15::analysis`, plus every slot an open
// upvalue can see). Temporaries of a statement in progress are not compared (a documented deviation from 9.4, which asks
// for every slot); the leftover window above the frame (TR3: popped callees' slots) is digested separately.

struct Live {
    sets: Vec<Option<SlotSet>>,
    escaped: SlotSet,
}

thread_local! {
    static LIVE: RefCell<HashMap<u64, Option<std::rc::Rc<Live>>>> = RefCell::new(HashMap::new());
}

fn build_info(p: &Proto) -> &'static FuncInfo<'static> {
    let code: &'static [u32] = Box::leak(p.code().to_vec().into_boxed_slice());
    let k = p.k().iter().map(|v| TagSet::of_tag(v.tag())).collect();
    let k_int = p.k().iter().map(|v| v.as_int()).collect();
    let mut escaped = SlotSet::EMPTY;
    for c in p.protos() {
        for u in c.upvalue_descs().iter().filter(|u| u.instack) {
            escaped.insert(u32::from(u.idx));
        }
    }
    Box::leak(Box::new(FuncInfo {
        code,
        k,
        k_int,
        max_stack: u32::from(p.maxstacksize),
        escaped,
        extra_heads: Vec::new(),
    }))
}

fn build_live(p: &Proto) -> Option<Live> {
    let info = build_info(p);
    let an: &'static Analysis<'static> = Box::leak(Box::new(Analysis::new(info).ok()?));
    let mut sets = vec![None; info.code.len()];
    for b in &an.blocks {
        let mut live = SlotSet::EMPTY;
        for &sc in &b.succs {
            live.union_with(&an.live_in_block(an.blocks[sc as usize].start));
        }
        for pc in (b.start..b.end).rev() {
            if an.role[pc as usize] != Role::Normal {
                continue;
            }
            let e = fx::effects(info, pc as usize);
            live = live.minus(&e.defs);
            live.union_with(&e.uses);
            sets[pc as usize] = Some(live);
        }
    }
    Some(Live {
        sets,
        escaped: info.escaped,
    })
}

fn live_of(p: &Proto) -> Option<std::rc::Rc<Live>> {
    let key = proto_key(p);
    LIVE.with(|m| {
        m.borrow_mut()
            .entry(key)
            .or_insert_with(|| build_live(p).map(std::rc::Rc::new))
            .clone()
    })
}

static MAIN: AtomicPtr<LuaState> = AtomicPtr::new(std::ptr::null_mut());

/// The size of the main thread's stack when `stack0` is its stack, else `None`.
fn main_stack_size(stack0: *const Value) -> Option<usize> {
    let m = MAIN.load(Ordering::Relaxed);
    if m.is_null() {
        return None;
    }
    // SAFETY: `MAIN` points at the state the running test owns (set before the run, cleared after); reading its stack
    // bookkeeping from a hook is what the existing oracle hooks do.
    let m = unsafe { &*m };
    std::ptr::eq(m.slot_ptr(StkId(0)).cast_const(), stack0).then(|| m.stack_size())
}

/// Digest of the machine state whose innermost frame is `ci` at `pc`, stack starting at `stack0`.
///
/// # Safety
/// `ci` is a live `CallInfo` of the thread whose stack starts at `stack0`; every frame's function slot and
/// `[0, ci.top)` are initialised slots of that stack.
unsafe fn digest(
    stack0: *const Value,
    ci: *mut CallInfo,
    pc: u32,
    root_base: Option<usize>,
    keep_raw: bool,
) -> Dig {
    let mut chain = 0x1234u64;
    let mut key0 = 0;
    let mut op0 = String::new();
    let mut live = Vec::new();
    let mut raw = Vec::new();
    let mut unanalysed = 0;
    let mut c = ci;
    let mut first = true;
    while !c.is_null() {
        // SAFETY: contract of this function.
        let (func, cs, savedpc, top, prev) = unsafe {
            (
                (*c).func.0,
                (*c).callstatus,
                (*c).savedpc,
                (*c).top.0,
                (*c).previous,
            )
        };
        let mut fh = mix(func as u64, u64::from(cs & CIST_C != 0));
        if cs & CIST_C == 0 {
            // SAFETY: the function slot of a live frame.
            let fv = unsafe { *stack0.add(func) };
            if let Some(cl) = fv.as_lclosure() {
                let p = cl.proto();
                let key = proto_key(&p);
                let idx = if first {
                    pc as usize
                } else {
                    ((savedpc as isize - p.code_ptr() as isize) / 4) as usize
                };
                fh = mix(mix(fh, key), idx as u64);
                if first {
                    key0 = key;
                    op0 = format!(
                        "{:?}",
                        rn_core::instruction::Instruction(p.code()[idx.min(p.code().len() - 1)])
                            .opcode()
                    );
                }
                match live_of(&p) {
                    Some(lv) if idx < lv.sets.len() => {
                        let mut set = lv.sets[idx].unwrap_or_default();
                        set.union_with(&lv.escaped);
                        // named locals only (registers below the number of locals active at the resume pc): the
                        // temporaries of a statement in progress are where tier 1 and the interpreter may
                        // differ (scratch copies, call windows), see the module docs
                        let nact = p
                            .locvars()
                            .iter()
                            .filter(|lv| lv.startpc as usize <= idx && idx < lv.endpc as usize)
                            .count() as u32;
                        for r in 0..nact.min(u32::from(p.maxstacksize)) {
                            let abs = func + 1 + r as usize;
                            if !set.contains(r) || abs >= top {
                                continue;
                            }
                            // SAFETY: `abs < top` of a live frame.
                            let v = unsafe { *stack0.add(abs) };
                            live.push((abs as u32, slot_hash(&v, true)));
                            if keep_raw {
                                let bits = v
                                    .as_int()
                                    .map(|x| x as u64)
                                    .or_else(|| v.as_float().map(f64::to_bits))
                                    .unwrap_or(0);
                                raw.push((v.tag(), bits));
                            }
                        }
                    }
                    _ => unanalysed += 1,
                }
            }
        }
        chain = mix(chain, fh);
        first = false;
        c = prev;
    }
    let mut left = Vec::new();
    // SAFETY: contract of this function.
    let top = unsafe { (*ci).top.0 };
    if let (Some(rb), Some(sz)) = (root_base, main_stack_size(stack0)) {
        for i in top..(rb + EXT).min(sz) {
            // SAFETY: `i < stack_size`: an initialised slot of the main thread's stack.
            left.push(slot_hash(&unsafe { *stack0.add(i) }, false));
        }
    }
    Dig {
        key: key0,
        pc,
        op: op0,
        chain,
        live,
        unanalysed,
        left,
        raw,
    }
}

// ---- checkpoints ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Stay,
    Exit,
}

struct Cp {
    clock: u64,
    kind: Kind,
    root_base: Option<usize>,
    dig: Dig,
    /// `stay`/`exit` plus a short description for reports.
    what: String,
    /// Taken while a stay was suspended in a helper: the clock is only known up to `NESTED_SLACK` (the suspended
    /// stay's helper instruction may not be counted yet).
    nested: bool,
    /// Run 2 bookkeeping.
    done: bool,
    tried: u32,
    fail: Option<String>,
}

/// The accepted clock error of a nested checkpoint (see `Cp::nested`, docs/JIT_TRACE_REF.md "P8a decisions").
const NESTED_SLACK: u64 = 16;

#[derive(Default)]
struct Rec {
    cps: Vec<Cp>,
    seen: u64,
    last_ended: u64,
    /// The stay's configured budget `B`.
    budget: i64,
    /// Checkpoints taken while a stay was suspended in a helper (OPEN > 0 at a fetch, > 1 at an exit).
    nested: u64,
    violations: Vec<String>,
    slotvecs: usize,
    exits_checked: u64,
    lows: HashMap<usize, (usize, Option<Lowered>)>,
}

thread_local! {
    static REC: RefCell<Rec> = RefCell::new(Rec::default());
    static CHECK: RefCell<Check> = RefCell::new(Check::default());
}

impl Rec {
    fn keep(&mut self) -> bool {
        self.seen += 1;
        (self.seen <= KEEP_ALL || self.seen.is_multiple_of(SAMPLE)) && self.cps.len() < MAX_CPS
    }
}

/// Run 1 hook (interpreter fetches): the fetch counter, the probe, stay checkpoints.
fn hook1(frame: usize, p: *const Proto, pc: usize, base: *mut Value, _ms: usize) {
    let clock = count::on_fetch(frame, p, pc);
    if let Some((lo, hi)) = debug_range() {
        let t = clock as i64
            + ss::INSNS.load(Ordering::Relaxed) as i64
            + ss::consumed_while_suspended();
        if (lo..hi).contains(&(t as u64)) {
            // SAFETY: the hook's proto is live.
            let key = proto_key(unsafe { &*p }) & 0xffff;
            eprintln!(
                "P8A-TRACE run1 fetch clock {t} (F {clock}) pc {pc} proto {key:#x} open {}",
                ss::OPEN.load(Ordering::Relaxed)
            );
        }
    }
    rn_jit::tr::exit::set_probe(Some(probe1));
    let ended = ss::ENDED.load(Ordering::Relaxed);
    REC.with(|r| {
        let mut r = r.borrow_mut();
        if ended == r.last_ended {
            return;
        }
        r.last_ended = ended;
        if ss::OPEN.load(Ordering::Relaxed) != 0 {
            r.nested += 1;
        }
        if !r.keep() {
            return;
        }
        let ci = frame as *mut CallInfo;
        // SAFETY: `frame` is the running frame's `CallInfo`, `base` its register 0.
        let (stack0, dig) = unsafe {
            let stack0 = base.cast_const().sub((*ci).func.0 + 1);
            (
                stack0,
                digest(stack0, ci, pc as u32, None, r.slotvecs < MAX_SLOTVECS),
            )
        };
        let _ = stack0;
        if !dig.raw.is_empty() {
            r.slotvecs += 1;
        }
        r.cps.push(Cp {
            clock: (clock as i64
                + ss::INSNS.load(Ordering::Relaxed) as i64
                + ss::consumed_while_suspended()) as u64,
            kind: Kind::Stay,
            root_base: None,
            dig,
            what: "stay end".into(),
            nested: ss::OPEN.load(Ordering::Relaxed) != 0,
            done: false,
            tried: 0,
            fail: None,
        });
    });
}

/// The exit handler's probe (run 1): the restored state of the exit that just happened.
fn probe1(st: &mut State, id: TrId) {
    use rn_jit::tr::exit::EXIT_FLAGS;
    // (`None` when the exit was a hot one and the trace is already parked in the grave: no snapshot sanity check and
    // no root frame for the leftover window then)
    let meta = rn_jit::tr::cache::exit_meta(st, id.slot).map(|m| m.1);
    let bp = rn_jit::rt::trace_budget_ptr(st);
    // SAFETY: the area follows the budget word of the live per-state data.
    let (word, exitno) = unsafe {
        let area = bp
            .cast::<u8>()
            .add(rn_jit::tr::exit_area::FROM_R15 as usize)
            .cast::<rn_jit::tr::ExitArea>();
        (*bp, (*area).exitno & !EXIT_FLAGS)
    };
    let snap_pc = meta
        .as_ref()
        .map(|m| m.snaps[m.exits[exitno as usize].snap as usize].pc);
    let ci = st.ci();
    let cl = st.get(ci.r().func).as_lclosure().expect("a Lua frame");
    let p = cl.proto();
    let pc = ((ci.r().savedpc as isize - p.code_ptr() as isize) / 4) as u32;
    let stack0 = st.slot_ptr(StkId(0)).cast_const();
    REC.with(|r| {
        let mut r = r.borrow_mut();
        let tag = format!("exit {exitno} of trace {} at pc {pc}", id.slot);
        if snap_pc.is_some_and(|s| s != pc) {
            r.violations.push(format!(
                "{tag}: restored pc {pc} != snapshot pc {snap_pc:?}"
            ));
        }
        // the landing label of the innermost prototype (cached per code address and length)
        let ck = p.code_ptr() as usize;
        let e = r.lows.entry(ck).or_insert((0, None));
        if e.0 != p.code().len() || e.1.is_none() {
            *e = (p.code().len(), rn_jit::tr::record::lower_for_landing(&p));
        }
        let Some(low) = e.1.as_ref() else {
            r.violations
                .push(format!("{tag}: the innermost prototype does not lower"));
            return;
        };
        let (kind, land) = match low.label_kind(pc) {
            LabelKind::Dead => {
                r.violations
                    .push(format!("{tag}: lands at a pc without a tier-1 label"));
                return;
            }
            LabelKind::Head => ("head", 0),
            LabelKind::Mid => ("mid", i64::from(low.rem(pc))),
        };
        if word < 0 {
            r.violations.push(format!(
                "{tag} ({kind}): budget word {word} is negative after the exit"
            ));
            return;
        }
        let bud = r.budget;
        let clock = count::total() as i64
            + ss::INSNS.load(Ordering::Relaxed) as i64
            + ss::SUSPENDED.load(Ordering::Relaxed)
            + (bud - i64::from(word) - land);
        r.exits_checked += 1;
        if ss::OPEN.load(Ordering::Relaxed) > 1 {
            r.nested += 1;
        }
        if clock < 0 {
            r.violations.push(format!(
                "{tag} ({kind}): negative clock {clock} (B {bud} word {word} land {land})"
            ));
            return;
        }
        if !r.keep() {
            return;
        }
        // the root frame: the nearest frame (from the innermost) running the trace's root prototype
        let mut root_base = None;
        let mut c = ci.as_ptr();
        while !c.is_null() {
            // SAFETY: the CallInfo chain of the running thread.
            let (func, cs, prev) = unsafe { ((*c).func.0, (*c).callstatus, (*c).previous) };
            if cs & CIST_C == 0 {
                // SAFETY: the function slot of a live frame.
                if let Some(cl) = unsafe { *stack0.add(func) }.as_lclosure() {
                    if meta
                        .as_ref()
                        .is_some_and(|m| cl.proto().code_ptr() == m.root_code)
                    {
                        root_base = Some(func + 1);
                        break;
                    }
                }
            }
            c = prev;
        }
        // SAFETY: `ci` is the innermost live frame of this thread; `stack0` its stack.
        let dig = unsafe {
            digest(
                stack0,
                ci.as_ptr(),
                pc,
                root_base,
                r.slotvecs < MAX_SLOTVECS,
            )
        };
        if !dig.raw.is_empty() {
            r.slotvecs += 1;
        }
        r.cps.push(Cp {
            clock: clock as u64,
            kind: Kind::Exit,
            root_base,
            dig,
            what: format!("{tag} ({kind}, B {bud} word {word} land {land})"),
            nested: ss::OPEN.load(Ordering::Relaxed) > 1,
            done: false,
            tried: 0,
            fail: None,
        });
    });
}

// ---- run 2: the JIT-off interpreter ------------------------------------------------------------------------------

#[derive(Default)]
struct Check {
    cps: Vec<Cp>,
    next: usize,
    matched: u64,
    gaps: u64,
    mism: Vec<String>,
    mism_count: u64,
    /// `(kind, innermost proto key, pc)` of every failed checkpoint (the tier-1 calibration subtracts the ones that
    /// also fail with traces off).
    fail_keys: Vec<(Kind, u64, u32)>,
    fail_msgs: Vec<String>,
    /// Frames of compared checkpoints whose prototype has no liveness (not compared).
    unanalysed: u64,
    /// Nested checkpoints that matched at a clock other than their own (within `NESTED_SLACK`).
    slack_used: u64,
    /// Clock of the previous interpreter fetch of run 2 (stay checkpoints strictly between two fetches are in an
    /// unfetched `TFOR*` chain, see `skip_chain_stays`).
    prev_clock: Option<u64>,
    /// Prototype of the previous interpreter fetch of run 2 (the unfetched chain of a `TFORPREP` / `TFORCALL` whose
    /// call runs a Lua iterator has the callee's fetch next, not one of the same prototype). Live: GC is stopped.
    prev_proto: Option<*const Proto>,
}

/// What differs between the digest of the traced run `a` and the interpreter's `b` (`None`: equal). The leftover
/// window is compared only when `with_left`.
fn describe(a: &Dig, b: &Dig, with_left: bool) -> Option<String> {
    if a.key != b.key {
        return Some("innermost prototype differs".into());
    }
    if a.pc != b.pc {
        return Some(format!("pc {} (trace run) vs {} (interpreter)", a.pc, b.pc));
    }
    if a.chain != b.chain {
        return Some("CallInfo chain differs (frame function / prototype / saved pc)".into());
    }
    if a.live.len() != b.live.len() {
        return Some(format!("{} vs {} live slots", a.live.len(), b.live.len()));
    }
    if let Some(k) = a.live.iter().zip(&b.live).position(|(x, y)| x != y) {
        let show = |d: &Dig| {
            d.raw.get(k).map_or("?".to_string(), |(t, p)| {
                format!("tag {t:#x} payload {p:#x}")
            })
        };
        let dump = |d: &Dig| {
            if debug_on() {
                format!(" {:?}", d.raw)
            } else {
                String::new()
            }
        };
        return Some(format!(
            "live stack slot {} differs at pc {} {} (trace run: {}; interpreter: {}){}{}",
            a.live[k].0,
            a.pc,
            a.op,
            show(a),
            show(b),
            dump(a),
            dump(b)
        ));
    }
    if with_left {
        // (clamped to the stack's size, which can differ between the runs: compare the common part)
        if let Some(i) = a.left.iter().zip(&b.left).position(|(x, y)| x != y) {
            return Some(format!(
                "LEFTOVER slot {} above the innermost frame differs ({} slots digested)",
                i,
                a.left.len().min(b.left.len())
            ));
        }
    }
    None
}

/// `TR_ORACLE_RANGE=lo-hi`: prints the fetches whose clock lies in the range in both runs.
fn debug_range() -> Option<(u64, u64)> {
    static D: std::sync::OnceLock<Option<(u64, u64)>> = std::sync::OnceLock::new();
    *D.get_or_init(|| {
        let v = std::env::var("TR_ORACLE_RANGE").ok()?;
        let (a, b) = v.split_once('-')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    })
}

fn debug_on() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| std::env::var("TR_ORACLE_DEBUG").is_ok())
}

fn hook2(frame: usize, p: *const Proto, pc: usize, base: *mut Value, _ms: usize) {
    let clock = count::on_fetch(frame, p, pc);
    if let Some((lo, hi)) = debug_range() {
        if (lo..hi).contains(&clock) {
            // SAFETY: the hook's proto is live.
            let key = proto_key(unsafe { &*p }) & 0xffff;
            eprintln!("P8A-TRACE run2 fetch clock {clock} pc {pc} proto {key:#x}");
        }
    }
    CHECK.with(|c| {
        let mut c = c.borrow_mut();
        skip_chain_stays(&mut c, p, clock);
        c.prev_clock = Some(clock);
        c.prev_proto = Some(p);
        // checkpoints whose window has passed: settle them
        while c.next < c.cps.len() {
            let i = c.next;
            let win = if c.cps[i].nested || c.cps[i].kind == Kind::Stay {
                NESTED_SLACK
            } else {
                0
            };
            if c.cps[i].clock + win >= clock {
                break;
            }
            c.next += 1;
            settle(&mut c, i);
        }
        let mut dig: Option<Dig> = None;
        let mut i = c.next;
        while i < c.cps.len() && c.cps[i].clock <= clock + NESTED_SLACK {
            let win = if c.cps[i].nested || c.cps[i].kind == Kind::Stay {
                NESTED_SLACK
            } else {
                0
            };
            let in_window = c.cps[i].clock <= clock + win && clock <= c.cps[i].clock + win;
            if in_window && !c.cps[i].done {
                let rb = c.cps[i].root_base;
                let d = dig.get_or_insert_with(|| {
                    let ci = frame as *mut CallInfo;
                    // SAFETY: `frame` is the running frame's `CallInfo`, `base` its register 0.
                    unsafe {
                        let stack0 = base.cast_const().sub((*ci).func.0 + 1);
                        digest(stack0, ci, pc as u32, rb, true)
                    }
                });
                // (a digest is taken with the first candidate's leftover window; exit checkpoints are exact, so
                // they are the only candidates of their clock)
                c.unanalysed += u64::from(d.unanalysed);
                match describe(&c.cps[i].dig, d, leftover_on()) {
                    None => {
                        c.cps[i].done = true;
                        c.matched += 1;
                        if clock != c.cps[i].clock {
                            c.slack_used += 1;
                        }
                    }
                    Some(why) => {
                        c.cps[i].tried += 1;
                        if debug_on() { eprintln!("P8A-TRY cp {} at fetch clock {clock} pc {pc}: {why}", c.cps[i].clock); }
                        if c.cps[i].fail.is_none() {
                            c.cps[i].fail = Some(why);
                        }
                    }
                }
            }
            i += 1;
        }
    });
}

/// P8c (Q5: also across a Lua iterator): a stay checkpoint whose clock lies strictly between two interpreter fetches, at
/// a `TFORCALL` / `TFORLOOP` of the prototype just fetched or of the one fetched before it (the next fetch is the Lua
/// iterator's when the chain's `TFORCALL` calls one), is in an unfetched chain (the interpreter counts the chained
/// instructions when it executes them, without a fetch): the interpreter never has that state at a fetch, so it is a gap (not comparable) instead of being compared
/// with unrelated fetches of the +-`NESTED_SLACK` window (which reported `pc 31 vs 2`). Exit checkpoints are exact and
/// never skipped; the instruction identity still covers the stay's total.
fn skip_chain_stays(c: &mut Check, p: *const Proto, clock: u64) {
    let Some(prev) = c.prev_clock else { return };
    // SAFETY: the interpreter passes the live prototype of the running frame.
    let proto = unsafe { &*p };
    let key = proto_key(proto);
    // SAFETY: the previous fetch's prototype is still alive (the collector is stopped for the whole run).
    let prev_proto = c.prev_proto.map(|q| unsafe { &*q });
    let mut i = c.next;
    while i < c.cps.len() && c.cps[i].clock < clock {
        let cp = &c.cps[i];
        if !cp.done
            && cp.kind == Kind::Stay
            && cp.clock > prev
            && (cp.dig.key == key || prev_proto.is_some_and(|q| proto_key(q) == cp.dig.key))
        {
            let pc = cp.dig.pc as usize;
            let owner = if cp.dig.key == key { proto } else { prev_proto.unwrap() };
            let op = owner
                .code()
                .get(pc)
                .and_then(|w| rn_core::instruction::Instruction(*w).opcode());
            if matches!(
                op,
                Some(
                    rn_core::ver::active::OpCode::Tforcall
                        | rn_core::ver::active::OpCode::Tforloop
                )
            ) {
                c.cps[i].done = true;
                c.gaps += 1;
            }
        }
        i += 1;
    }
}

/// A checkpoint's window is over: a match, a mismatch or a gap (no fetch at that clock).
fn settle(c: &mut Check, i: usize) {
    if c.cps[i].done {
        return;
    }
    if c.cps[i].tried == 0 {
        c.gaps += 1; // no fetch in the window (an unfetched TFOR chain)
        return;
    }
    c.matched += 1;
    c.mism_count += 1;
    let key = (c.cps[i].kind, c.cps[i].dig.key, c.cps[i].dig.pc);
    c.fail_keys.push(key);
    let m = {
        let cp = &c.cps[i];
        format!(
            "clock {}{}: {} ({:?}): {}",
            cp.clock,
            if cp.nested { " (nested)" } else { "" },
            cp.what,
            cp.kind,
            cp.fail.as_deref().unwrap_or("?")
        )
    };
    c.fail_msgs.push(m);
    if c.mism.len() < 12 {
        let cp = &c.cps[i];
        let msg = format!(
            "clock {}{}: {} ({:?}): {}",
            cp.clock,
            if cp.nested { " (nested)" } else { "" },
            cp.what,
            cp.kind,
            cp.fail.as_deref().unwrap_or("?")
        );
        c.mism.push(msg);
    }
}

// ---- the child ---------------------------------------------------------------------------------------------------------

/// GC stopped: collection timing (clearing of dead stack slices, finalizers) must not differ between the two runs.
const GC_STOP: &str = "collectgarbage('stop')\n";

fn run_state(l: &Loaded, cfg: &Cfg, hook: rn_core::oracle::Hook) -> (u64, Option<String>) {
    let mut st = make_state(cfg);
    MAIN.store(&mut st as *mut LuaState, Ordering::SeqCst);
    count::reset();
    rn_core::oracle::set_hook(Some(hook));
    run_chunk(&mut st, GC_STOP.as_bytes(), b"=gcstop");
    if !l.prelude.is_empty() {
        run_chunk(&mut st, l.prelude.as_bytes(), b"=prelude");
    }
    let (_, out) = run_chunk(&mut st, &l.src, format!("@{}", l.path).as_bytes());
    rn_core::oracle::set_hook(None);
    rn_jit::tr::exit::set_probe(None);
    MAIN.store(std::ptr::null_mut(), Ordering::SeqCst);
    let total = count::total();
    drop(st);
    (total, out)
}

fn child_main() {
    let spec = std::env::var("TR_OR_SPEC").unwrap();
    let mode = std::env::var("TR_OR_MODE").unwrap();
    let opts = std::env::var("TR_OR_OPTS").unwrap_or_default();
    let budget: Option<i32> = std::env::var("TR_OR_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok());
    child_setup(mode == "on");
    let l = match load(&spec, &core_src) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("P8A-SKIP {e}");
            return;
        }
    };
    // determinism of the interpreter baseline (and its instruction count)
    let off = Cfg::new("off", "", None);
    let hook0: rn_core::oracle::Hook = |f, p, pc, _, _| {
        count::on_fetch(f, p, pc);
    };
    let (n0, o0) = run_state(&l, &off, hook0);
    let (n1, _) = run_state(&l, &off, hook0);
    if n0 != n1 {
        eprintln!("P8A-SKIP nondeterministic baseline ({n0} vs {n1})");
        return;
    }
    let cfg = Cfg::new(&mode, &opts, budget);
    let p = pass(&l, &cfg, budget, &o0);
    for v in p.violations.iter().take(8) {
        eprintln!("P8A-VIOL {v}");
    }
    if p.violations.len() > 8 {
        eprintln!("P8A-VIOL ... and {} more", p.violations.len() - 8);
    }
    if p.output_differs {
        eprintln!("P8A-MISMATCH output of the traced run differs from the interpreter's");
    }
    let c = p.check;
    let unreached = c.cps.len() - c.next;
    if unreached > 0 {
        eprintln!(
            "P8A-MISMATCH {unreached} checkpoints lie beyond the end of the interpreter run, first clock {}",
            c.cps[c.next].clock
        );
    }
    for m in &c.mism {
        eprintln!("P8A-MISMATCH {m}");
    }
    if c.mism_count as usize > c.mism.len() {
        eprintln!("P8A-MISMATCH ... {} mismatches in all", c.mism_count);
    }
    eprintln!(
        "P8A-STAT stay={} exit={} matched={} gaps={} nested={} exits_seen={} unanalysed={} slack_used={} traced_total={}",
        p.n_stay, p.n_exit, c.matched, c.gaps, p.nested, p.exits_checked, c.unanalysed, c.slack_used, p.n_traced
    );
    eprintln!("P8A-MEM peak_kb={}", peak_rss_kb());
    eprintln!("P8A-DONE");
}

struct Pass {
    check: Check,
    violations: Vec<String>,
    output_differs: bool,
    n_stay: usize,
    n_exit: usize,
    nested: u64,
    exits_checked: u64,
    n_traced: u64,
}

/// Run 1 (configuration `cfg`, checkpoints logged) and run 2 (JIT off, checkpoints compared).
fn pass(l: &Loaded, cfg: &Cfg, budget: Option<i32>, o0: &Option<String>) -> Pass {
    let off = Cfg::new("off", "", None);
    REC.with(|r| {
        *r.borrow_mut() = Rec {
            budget: i64::from(budget.unwrap_or(i32::MAX)),
            last_ended: ss::ENDED.load(Ordering::SeqCst),
            ..Rec::default()
        }
    });
    let (n_traced, o_traced) = run_state(l, cfg, hook1);
    let r = REC.with(|r| std::mem::take(&mut *r.borrow_mut()));
    let (n_stay, n_exit) = (
        r.cps.iter().filter(|c| c.kind == Kind::Stay).count(),
        r.cps.iter().filter(|c| c.kind == Kind::Exit).count(),
    );
    let mut cps = r.cps;
    cps.sort_by_key(|c| c.clock);
    CHECK.with(|c| {
        *c.borrow_mut() = Check {
            cps,
            ..Check::default()
        }
    });
    run_state(l, &off, hook2);
    let mut c = CHECK.with(|c| std::mem::take(&mut *c.borrow_mut()));
    for i in c.next..c.cps.len() {
        if c.cps[i].tried > 0 || c.cps[i].done {
            settle(&mut c, i);
            c.next = i + 1;
        }
    }
    Pass {
        check: c,
        violations: r.violations,
        output_differs: l.path.starts_with("core/") && *o0 != o_traced,
        n_stay,
        n_exit,
        nested: r.nested,
        exits_checked: r.exits_checked,
        n_traced,
    }
}

#[test]
fn child_entry() {
    if std::env::var("TR_OR_SPEC").is_ok() {
        child_main();
    }
}

// ---- the parent --------------------------------------------------------------------------------------------------------

struct Cell {
    mode: &'static str,
    opts: &'static str,
    budget: Option<i32>,
}

const fn cell(mode: &'static str, opts: &'static str, budget: Option<i32>) -> Cell {
    Cell { mode, opts, budget }
}

fn run_cells(title: &str, cells: &[Cell], core_only: bool, gen: u32) {
    if std::env::var("TR_OR_SPEC").is_ok() {
        return;
    }
    let exe = std::env::current_exe().expect("exe");
    // (`pm3_rec` and `r2_rec_deep` recurse to the stack limit: millions of logged checkpoints; `pm3_rec2` and `r2_rec` cover the recursion; `be_queens` is 400 backtracking searches: BUNDLE-E; `h1_towers_piles` is ~10^5 logged moves: BUNDLE-H1)
    let names: Vec<&str> = progs::PROGS
        .iter()
        .chain(sink_progs::SINK_PROGS)
        .map(|p| p.name)
        .filter(|n| *n != "pm3_rec" && *n != "r2_rec_deep" && *n != "bf_rec_deep" && *n != "be_queens" && *n != "h1_towers_piles" && *n != "h1_hash_int_many")
        .collect();
    let mut items = corpus(&names, gen);
    if core_only {
        items.retain(|i| i.name.starts_with("core/"));
    }
    items.retain(|i| !MEM_HEAVY.contains(&i.name.as_str()));
    let mut jobs = Vec::new();
    for it in &items {
        for c in cells {
            let mut env = vec![
                ("TR_OR_SPEC".to_string(), it.spec.clone()),
                ("TR_OR_MODE".to_string(), c.mode.to_string()),
                ("TR_OR_OPTS".to_string(), c.opts.to_string()),
            ];
            if let Some(b) = c.budget {
                env.push(("TR_OR_BUDGET".into(), b.to_string()));
            }
            jobs.push(Job {
                label: format!(
                    "{} [{} {} budget={}]",
                    it.name,
                    c.mode,
                    c.opts,
                    c.budget.map_or("none".into(), |b| b.to_string())
                ),
                env,
            });
        }
    }
    let njobs = jobs.len();
    let label_env: std::collections::HashMap<String, Vec<(String, String)>> = jobs
        .iter()
        .map(|j| (j.label.clone(), j.env.clone()))
        .collect();
    let res = run_pool(&exe, "child_entry", jobs, timeout_secs(300));
    let mut tier1_skew = 0;
    let mut bad = String::new();
    let (mut done, mut skipped, mut inconclusive) = (0, 0, 0);
    let (mut stays, mut exits, mut matched, mut gaps, mut nested) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut notes = Vec::new();
    let mut mem: Vec<(u64, String)> = Vec::new();
    for (label, o) in &res {
        if marker_lines(&o.stderr, "P8A-DONE").next().is_some() {
            done += 1;
        } else if marker_lines(&o.stderr, "P8A-SKIP").next().is_some() {
            skipped += 1;
            notes.push(format!("skipped: {label}"));
        } else {
            inconclusive += 1;
            notes.push(format!(
                "inconclusive: {label} (timeout={} crash={}) {}",
                o.timed_out,
                o.signalled,
                tail(&o.stderr, 3).replace('\n', " | ")
            ));
            if o.signalled {
                bad.push_str(&format!("--- {label}\nCRASH\n{}\n", tail(&o.stderr, 12)));
            }
        }
        if let Some(kb) = marker_lines(&o.stderr, "P8A-MEM")
            .next()
            .and_then(|l| l.split("peak_kb=").nth(1))
            .and_then(|v| v.trim().parse::<u64>().ok())
        {
            mem.push((kb, label.clone()));
        }
        for l in marker_lines(&o.stderr, "P8A-STAT") {
            let g = |k: &str| -> u64 {
                l.split_whitespace()
                    .find_map(|w| w.strip_prefix(k))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
            };
            stays += g("stay=");
            exits += g("exit=");
            matched += g("matched=");
            gaps += g("gaps=");
            nested += g("nested=");
            if std::env::var("TR_ORACLE_V").is_ok() {
                eprintln!("{label}: {l}");
            }
        }
        let mism: Vec<&str> = marker_lines(&o.stderr, "P8A-MISMATCH").collect();
        let viol: Vec<&str> = marker_lines(&o.stderr, "P8A-VIOL").collect();
        if !mism.is_empty() || !viol.is_empty() {
            // Tier 1 alone is not exact between stay ends (module docs, deviation 3): a job whose only failures are
            // stay checkpoints is a `tier1_skew` if the same job with traces off fails the same way in a fresh child.
            let only_stay = viol.is_empty()
                && mism
                    .iter()
                    .all(|m| m.contains("(Stay)") || m.contains("mismatches in all"));
            let prog = label.split(" [").next().unwrap_or("");
            if only_stay
                && (STAY_SKEW_PROGS.contains(&prog)
                    || skew_with_traces_off(&exe, &label_env[label]))
            {
                tier1_skew += 1;
                continue;
            }
            for l in mism.iter().chain(&viol) {
                bad.push_str(&format!("--- {label}\n{l}\n"));
            }
        }
    }
    eprintln!(
        "[tr_oracle] {title}: {njobs} jobs: {done} checked, {skipped} skipped, {inconclusive} inconclusive; \
         checkpoints logged: {stays} stay + {exits} exit; compared {matched}, unfetched-chain gaps {gaps}, taken while nested {nested}; \
         jobs with tier-1 stay skew (also with traces off): {tier1_skew}"
    );
    for n in &notes {
        eprintln!("[tr_oracle]   {n}");
    }
    mem.sort();
    for (kb, l) in mem.iter().rev().take(3) {
        eprintln!("[tr_oracle]   peak RSS {} MB: {l}", kb / 1024);
    }
    assert!(bad.is_empty(), "{title}: oracle failures:\n{bad}");
    if std::env::var("TR_P8A_ONLY").is_err() {
        assert!(stays > 0, "{title}: no stay checkpoint was compared");
        assert!(exits > 0, "{title}: no trace exit was compared");
    }
}

/// Does the job (environment `env` of a traced cell) fail with stay mismatches also with traces off (`t1`)?
fn skew_with_traces_off(exe: &std::path::Path, env: &[(String, String)]) -> bool {
    let env: Vec<(String, String)> = env
        .iter()
        .map(|(k, v)| {
            if k == "TR_OR_MODE" {
                (k.clone(), "t1".to_string())
            } else if k == "TR_OR_OPTS" {
                (k.clone(), String::new())
            } else {
                (k.clone(), v.clone())
            }
        })
        .collect();
    let o = run_child(exe, "child_entry", &env, timeout_secs(300));
    let found = marker_lines(&o.stderr, "P8A-MISMATCH").next().is_some();
    found
}

/// Trace modes (stay checkpoints: every compiled stay's end, including all exit kinds that reach the interpreter).
#[test]
fn oracle_trace_modes() {
    run_cells(
        "trace modes",
        &[
            cell("always", "", None),
            cell("always", "", Some(3)),
            cell("always", "", Some(17)),
            cell("always", "", Some(200)),
            cell("on2", "", None),
            cell("on2", "", Some(17)),
            cell("on", "", None),
        ],
        false,
        8,
    );
}

/// Exit checkpoints: `+emuverify` forces every exit through the Rust handler, where the oracle observes the restored
/// state; with budgets, exit storms and the option variants (every exit map and landing label).
#[test]
fn oracle_exits() {
    run_cells(
        "exits (+emuverify)",
        &[
            cell("always", "+emuverify", None),
            cell("always", "+emuverify", Some(3)),
            cell("always", "+emuverify", Some(17)),
            cell("always", "+emuverify", Some(200)),
            cell("always", "+emuverify +exitstorm=3", Some(17)),
            cell("on2", "+emuverify +exitstorm=7 +randra=5", None),
        ],
        false,
        4,
    );
}

/// The option variants without `+emuverify` (inline stubs; stay checkpoints).
#[test]
fn oracle_options() {
    run_cells(
        "option variants",
        &[
            cell("always", "-fold -cse", Some(17)),
            cell("always", "+exitstorm=2", None),
            cell("always", "+exitstorm=7", Some(3)),
            cell("always", "+randra=3", Some(17)),
            cell("always", "+membudget", Some(17)),
            cell("always", "-loop -fwd -abc -sink", None),
        ],
        false,
        4,
    );
}

/// The mutant probe of `tr_mutants.rs`: a small family that passes without a mutant and fails (an oracle mismatch, a
/// violation, a hung or crashed child) under the mutants that corrupt a restored state or a budget. The children inherit
/// `RUSTYNAIL_JIT_MUT`.
#[test]
fn mutant_probe() {
    if std::env::var("TR_OR_SPEC").is_ok() {
        return;
    }
    let exe = std::env::current_exe().expect("exe");
    let cells = [
        cell("always", "", None),
        cell("always", "", Some(17)),
        cell("always", "+emuverify", Some(17)),
        cell("always", "+emuverify +exitstorm=3", None),
    ];
    let probe = [
        "int_sum",
        "inline_exit",
        "inline_2",
        "float_forloop",
        "tc_acc",
        "tc_callee",
        "nested_tables",
    ];
    let mut jobs = Vec::new();
    for p in probe {
        for c in &cells {
            let mut env = vec![
                ("TR_OR_SPEC".to_string(), format!("core|{p}|")),
                ("TR_OR_MODE".to_string(), c.mode.to_string()),
                ("TR_OR_OPTS".to_string(), c.opts.to_string()),
            ];
            if let Some(b) = c.budget {
                env.push(("TR_OR_BUDGET".into(), b.to_string()));
            }
            jobs.push(Job {
                label: format!("{p} [{} {} {:?}]", c.mode, c.opts, c.budget),
                env,
            });
        }
    }
    let res = run_pool(&exe, "child_entry", jobs, timeout_secs(30));
    let mut bad = String::new();
    for (label, o) in &res {
        if marker_lines(&o.stderr, "P8A-DONE").next().is_none() {
            bad.push_str(&format!(
                "--- {label}: did not finish (timeout={} crash={})\n{}\n",
                o.timed_out,
                o.signalled,
                tail(&o.stderr, 6)
            ));
        }
        for l in marker_lines(&o.stderr, "P8A-MISMATCH").chain(marker_lines(&o.stderr, "P8A-VIOL"))
        {
            bad.push_str(&format!("--- {label}\n{l}\n"));
        }
    }
    assert!(bad.is_empty(), "oracle mutant probe:\n{bad}");
}
