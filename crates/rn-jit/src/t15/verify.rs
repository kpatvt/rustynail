//! Verification tooling of tier-1.5 regions (task T8; docs/JIT_TIER15.md sections 7 items 4-6 and 22).
//!
//! Two runtime options, never on by default (`RUSTYNAIL_JIT_OPT`), both only changing the *code emitted for
//! regions*:
//!
//! * **`+exitstorm=N`**: pseudo-guards that fail every N-th execution ([`storm_counter`]: a process-wide table of
//!   32-bit counters, one per site; the sites are chosen by `codegen.rs` and `stubs.rs`).
//! * **`+shadow`**: shadow verification of side-effect-free region blocks. Before such a block the generated code
//!   flushes and calls [`jit_t15_shadow_pre`] (copies the frame slots and the budget word to a thread-local shadow
//!   buffer); at every successor edge it flushes and calls [`jit_t15_shadow_post`], which runs the **tier-1 code of
//!   the same block in the emulator** on the shadow copy and compares the whole frame, the budget word and the
//!   successor with what the region code produced. A mismatch prints the block, the slot and both values and
//!   aborts. A block whose emulated tier-1 code needs a helper (slow path), leaves through an exit or does not
//!   finish within the step limit is *inconclusive* and counted as such.
//!
//! The registry of blocks lives in a process-wide table (never freed: this is a debugging aid, and code that was
//! retired may still run). Every entry gets its tier-1 [`Program`] after the lowering finished
//! ([`finish_lowering`]), because the region hook runs before the out-of-line tier-1 code (slow paths, exits) exists.

use crate::emu::{label_table_of, run_until_shared, Env, Until};
use crate::ir::{Program, L};
use rn_core::jit_hooks;
use rn_core::state::{CallInfo, Ci, StkId, Thread};
use rn_core::value::{LUA_VNUMFLT, LUA_VNUMINT};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

// ---- exit storm ------------------------------------------------------------------------------------------

const STORM_SLOTS: usize = 1 << 15;
static STORM: [AtomicU32; STORM_SLOTS] = [const { AtomicU32::new(0) }; STORM_SLOTS];
static STORM_NEXT: AtomicU32 = AtomicU32::new(0);

/// The address of a fresh pseudo-guard counter (sites beyond the table size share counters, which only changes
/// their phase). The generated code increments the word non-atomically; nothing else reads it.
pub fn storm_counter() -> usize {
    let i = STORM_NEXT.fetch_add(1, Ordering::Relaxed) as usize % STORM_SLOTS;
    std::ptr::from_ref(&STORM[i]) as usize
}

// ---- shadow verification ---------------------------------------------------------------------------------

/// The tier-1 program of one lowering plus what the emulation needs to run blocks of it.
pub struct ShadowProg {
    prog: Program,
    labels: Arc<Vec<usize>>,
    /// Labels at which a block's emulation ends, and the pc they stand for (tier-1 block heads, region entry stubs).
    stops: Vec<(L, u32)>,
    stop_labels: Vec<L>,
}

/// A block that is verified.
pub struct ShadowBlock {
    /// First pc of the block.
    pub start_pc: u32,
    /// Last pc of the block.
    pub end_pc: u32,
    /// Tier-1 label of the first pc.
    pub start: L,
    /// `maxstacksize`.
    pub max_stack: u32,
    prog: OnceLock<Arc<ShadowProg>>,
}

static REGISTRY: Mutex<Vec<Arc<ShadowBlock>>> = Mutex::new(Vec::new());
static CHECKED: AtomicU64 = AtomicU64::new(0);
static INCONCLUSIVE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Pending {
    ids: Vec<u32>,
    stops: Vec<(L, u32)>,
    heads_added: bool,
}

thread_local! {
    static PENDING: RefCell<Pending> = RefCell::new(Pending::default());
    static SHADOW: RefCell<Shadow> = const { RefCell::new(Shadow { id: u32::MAX, slots: Vec::new(), budget: 0 }) };
}

struct Shadow {
    id: u32,
    slots: Vec<u8>,
    budget: i32,
}

/// Slots of padding in front of / behind the shadow frame (templates of verified blocks touch none of them).
const PAD_SLOTS: usize = 4;

/// Registers a block of the lowering in progress; returns its id (the `a` argument of the helper calls).
pub fn register_block(start_pc: u32, end_pc: u32, start: L, max_stack: u32) -> u32 {
    let b = Arc::new(ShadowBlock {
        start_pc,
        end_pc,
        start,
        max_stack,
        prog: OnceLock::new(),
    });
    let mut r = REGISTRY.lock().expect("shadow registry");
    r.push(b);
    let id = (r.len() - 1) as u32;
    PENDING.with(|p| p.borrow_mut().ids.push(id));
    id
}

/// Adds the tier-1 block heads `(label, pc)` of the function being lowered (once per lowering).
pub fn add_heads(heads: impl Iterator<Item = (L, u32)>) {
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        if !p.heads_added {
            p.heads_added = true;
            p.stops.extend(heads);
        }
    });
}

/// Adds the entry stub label of a region head (tier-1 jumps to the head are redirected to it).
pub fn add_stub_head(l: L, pc: u32) {
    PENDING.with(|p| p.borrow_mut().stops.push((l, pc)));
}

/// Called when a lowering ended: attaches its final tier-1 program to the blocks registered since the last call.
pub fn finish_lowering(prog: Option<&Program>) {
    let (ids, stops) = PENDING.with(|p| {
        let mut p = p.borrow_mut();
        let r = (std::mem::take(&mut p.ids), std::mem::take(&mut p.stops));
        p.heads_added = false;
        r
    });
    let Some(prog) = prog.filter(|_| !ids.is_empty()) else {
        return;
    };
    let sp = Arc::new(ShadowProg {
        labels: Arc::new(label_table_of(prog)),
        prog: prog.clone(),
        stop_labels: stops.iter().map(|&(l, _)| l).collect(),
        stops,
    });
    let r = REGISTRY.lock().expect("shadow registry");
    for id in ids {
        let _ = r[id as usize].prog.set(Arc::clone(&sp));
    }
}

/// `(checked, inconclusive)` block executions so far in this process.
pub fn shadow_counts() -> (u64, u64) {
    (
        CHECKED.load(Ordering::Relaxed),
        INCONCLUSIVE.load(Ordering::Relaxed),
    )
}

fn block(id: u32) -> Arc<ShadowBlock> {
    Arc::clone(&REGISTRY.lock().expect("shadow registry")[id as usize])
}

/// The frame base (`rbx`) of the running Lua frame.
///
/// # Safety
/// `th`, `ci` are the live thread and its current frame.
unsafe fn frame_of(th: *mut Thread, ci: *mut CallInfo) -> (rn_core::state::State, Ci, *mut u8) {
    // SAFETY: per the contract.
    let (st, ci) = unsafe { (jit_hooks::state_from_thread(th.cast()), Ci::from_raw(ci)) };
    let func = ci.r().func.0;
    let base = st.slot_ptr(StkId(func + 1)).cast::<u8>();
    (st, ci, base)
}

/// `jit_t15_shadow_pre(thread, ci, block, _)`: copies the frame `[0, maxstacksize)` and the budget word of the
/// running frame to the thread's shadow buffer. Called with everything flushed.
///
/// # Safety
/// Called only from compiled code (region code of `+shadow`) with the running thread and its current frame.
pub unsafe extern "C" fn jit_t15_shadow_pre(
    th: *mut Thread,
    ci: *mut CallInfo,
    id: u32,
    _b: u32,
) -> u32 {
    let blk = block(id);
    // SAFETY: per the contract.
    let (mut st, _ci, base) = unsafe { frame_of(th, ci) };
    let n = blk.max_stack as usize * 16;
    SHADOW.with(|s| {
        let mut s = s.borrow_mut();
        s.id = id;
        s.slots.clear();
        s.slots
            .resize((blk.max_stack as usize + 2 * PAD_SLOTS) * 16, 0);
        // SAFETY: `base` points at the frame, which has at least `maxstacksize` slots; the copy target is large enough.
        unsafe {
            std::ptr::copy_nonoverlapping(base, s.slots.as_mut_ptr().add(PAD_SLOTS * 16), n);
        }
        // SAFETY: the per-state data is live while compiled code runs.
        s.budget = unsafe { *crate::rt::cnt_ptr(&mut st) };
    });
    0
}

fn desc(tag: u8, bits: u64) -> String {
    match tag {
        LUA_VNUMINT => format!("int {}", bits as i64),
        LUA_VNUMFLT => format!("float {} (bits {bits:#018x})", f64::from_bits(bits)),
        t => format!("tag {t:#04x} payload {bits:#018x}"),
    }
}

/// Is the payload of a value with this tag significant (the other tags leave it unspecified)?
fn payload_matters(tag: u8) -> bool {
    tag == LUA_VNUMINT || tag == LUA_VNUMFLT || tag & 0x40 != 0
}

/// `jit_t15_shadow_post(thread, ci, block, successor_pc)`: runs the tier-1 code of the block on the shadow copy
/// and compares the frame, the budget word and the successor with the live (flushed) state.
///
/// # Safety
/// As [`jit_t15_shadow_pre`]; additionally `jit_t15_shadow_pre` of the same block ran before.
pub unsafe extern "C" fn jit_t15_shadow_post(
    th: *mut Thread,
    ci: *mut CallInfo,
    id: u32,
    succ: u32,
) -> u32 {
    let blk = block(id);
    let Some(sp) = blk.prog.get() else {
        INCONCLUSIVE.fetch_add(1, Ordering::Relaxed);
        return 0;
    };
    // SAFETY: per the contract.
    let (mut st, cir, base) = unsafe { frame_of(th, ci) };
    // SAFETY: the per-state data is live.
    let live_budget = unsafe { *crate::rt::cnt_ptr(&mut st) };
    let kcache = cir.r().kcache;
    SHADOW.with(|s| {
        let mut s = s.borrow_mut();
        assert_eq!(
            s.id, id,
            "[t15] shadow verification: post of block {id} without its pre"
        );
        let mut budget = s.budget;
        let env = Env {
            // SAFETY: inside the buffer (`PAD_SLOTS` slots of padding in front).
            frame: unsafe { s.slots.as_mut_ptr().add(PAD_SLOTS * 16) },
            ci: ci.cast(),
            thread: th.cast(),
            k: kcache.cast(),
            cnt: &raw mut budget,
        };
        // SAFETY: the emulator runs tier-1 templates of a block that makes no helper call (else it stops); the
        // frame is the shadow buffer, the `CallInfo`, the thread and the constants are the live ones (read only
        // by pure blocks), the budget word is the local copy.
        let r = unsafe {
            run_until_shared(
                &sp.prog,
                &sp.labels,
                blk.start,
                &sp.stop_labels,
                &env,
                20_000,
            )
        };
        let Until::Label(l) = r else {
            INCONCLUSIVE.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let emu_succ = sp.stops.iter().find(|&&(x, _)| x == l).map(|&(_, pc)| pc);
        let fail = |what: String| -> ! {
            eprintln!(
                "[t15] SHADOW VERIFICATION FAILED in block [{}, {}] (successor {succ}): {what}",
                blk.start_pc, blk.end_pc
            );
            std::process::abort()
        };
        if emu_succ != Some(succ) {
            fail(format!(
                "region continues at pc {succ}, tier 1 at {emu_succ:?}"
            ));
        }
        if budget != live_budget {
            fail(format!(
                "budget word: region {live_budget}, tier 1 {budget} (before: {})",
                s.budget
            ));
        }
        for slot in 0..blk.max_stack as usize {
            // SAFETY: both frames have `max_stack` slots of 16 bytes.
            let (rt, rp, et, ep) = unsafe {
                let r = base.add(slot * 16);
                let e = env.frame.add(slot * 16);
                (
                    r.read(),
                    r.add(8).cast::<u64>().read_unaligned(),
                    e.read(),
                    e.add(8).cast::<u64>().read_unaligned(),
                )
            };
            if rt != et || (payload_matters(et) && rp != ep) {
                fail(format!(
                    "slot {slot}: region {}, tier 1 {}",
                    desc(rt, rp),
                    desc(et, ep)
                ));
            }
        }
        CHECKED.fetch_add(1, Ordering::Relaxed);
    });
    0
}
