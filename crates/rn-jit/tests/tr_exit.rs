//! J5-P4: exits and the restore of inlined frames (docs/JIT_TRACE.md 5.4-5.6, 7.6, TR3, TR3a, TR5.4, TR18; task P4,
//! decisions in docs/JIT_TRACE_REF.md "P4 decisions").
//!
//! 1. `restore_*`: the Rust restore on synthetic memory against an explicit model of 5.5, for 0..4 inlined frames
//!    (Miri-runnable);
//! 2. `build_*`: snapshots resolved from a `TraceIr` and P5a's allocation, restored from a simulated register file;
//! 3. `twin_*`: the restore against the REAL interpreter: a Lua call chain is run until a native `cap` is reached
//!    inside the innermost call, the whole observable state (stack slots, `CallInfo` chain, `savedpc`, `callstatus`,
//!    `kcache`, `thread.ci/top`) is captured, then the frames are popped/scrubbed and rebuilt by the restore from a
//!    snapshot derived from that capture; the rebuilt state must equal the capture (popped-frame slots included);
//! 4. `shared_*`/`slice_*`: the shared exit routine in the emulator, and the P0 slice with generic stubs, inline
//!    stubs and tier 1 alone (landing label and budget exactness against the count model);
//! 5. `flush_*`: the handler with real traces (fake machine code): exit counting, hot reporting, flushes deferred.
//!
//! Mutants (feature `tr_mutants`): `frame_savedpc_caller_off` and `frame_nresults_off`; `mutants_are_caught` runs
//! the named tests in a child process with the mutant selected and requires them to fail.
#![cfg(feature = "trace")]

use rn_core::state::{CallInfo, Ci, LuaState, State, StkId, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::value::*;
use rn_jit::tr::exit::*;
use rn_jit::tr::ir::*;
use rn_jit::tr::ra::{self, Allocation, Loc};
use rn_jit::tr::{exit_area, ExitArea};
use rn_jit::{Config, Mode};
use std::cell::RefCell;

/// F6: the tests leak programs / strings on purpose (`&'static`); recording the address in a static keeps the memory *reachable*, so
/// LeakSanitizer (scripts/asan-jit.sh) does not report it.
static KEPT: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
fn keep<T: ?Sized>(r: &'static T) -> &'static T {
    KEPT.lock().unwrap_or_else(|e| e.into_inner()).push((r as *const T).cast::<u8>() as usize);
    r
}

// ---- small helpers ----------------------------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, a: u64, b: u64) -> bool {
        self.below(b) < a
    }
}

/// The budget word followed by the exit area, as `r15` addresses them (5.6).
#[repr(C)]
#[derive(Clone)]
struct Bud {
    cnt: i32,
    _pad: u32,
    area: ExitArea,
}

impl Bud {
    fn new() -> Box<Bud> {
        Box::new(Bud {
            cnt: 0,
            _pad: 0,
            area: ExitArea::default(),
        })
    }
}

#[test]
fn bud_layout_matches_the_frozen_area_offset() {
    let b = Bud::new();
    let base = std::ptr::addr_of!(b.cnt) as usize;
    let area = std::ptr::addr_of!(b.area) as usize;
    assert_eq!(area - base, exit_area::FROM_R15 as usize);
}

fn zeroed_ci() -> CallInfo {
    // SAFETY: `CallInfo` is plain data (integers, raw pointers, an `Option` of a fn pointer): all-zero is a valid value.
    unsafe { std::mem::zeroed() }
}

const SUPPORTED_TAGS: [u8; 14] = [
    LUA_VNIL,
    LUA_VFALSE,
    LUA_VTRUE,
    LUA_VNUMINT,
    LUA_VNUMFLT,
    LUA_VSHRSTR | (1 << 6),
    LUA_VLNGSTR | (1 << 6),
    LUA_VTABLE | (1 << 6),
    LUA_VLCF,
    LUA_VLCL | (1 << 6),
    LUA_VCCL | (1 << 6),
    LUA_VUSERDATA | (1 << 6),
    LUA_VLIGHTUSERDATA,
    LUA_VTHREAD | (1 << 6),
];

// ---- 1. restore on synthetic memory vs the 5.5 model ------------------------------------------------------------

const NSTACK: usize = 200;
const ROOT_FUNC: usize = 3;
const POISON: u64 = 0xDEAD_BEEF_0BAD_F00D;

/// The `CallInfo` nodes of a synthetic thread: a leaked boxed slice (raw pointers stay valid when the owner moves;
/// moving a `Box` would retag it under Stacked Borrows and invalidate the links between the nodes).
struct Nodes {
    p: *mut CallInfo,
    n: usize,
}

impl Nodes {
    fn new(n: usize) -> Nodes {
        let b: Box<[CallInfo]> = (0..n).map(|_| zeroed_ci()).collect();
        Nodes {
            p: Box::into_raw(b).cast(),
            n,
        }
    }
    fn as_mut_ptr(&self) -> *mut CallInfo {
        self.p
    }
    fn iter(&self) -> std::slice::Iter<'_, CallInfo> {
        // SAFETY: `p` points at `n` initialised nodes owned by `self`.
        unsafe { std::slice::from_raw_parts(self.p, self.n) }.iter()
    }
}

impl std::ops::Index<usize> for Nodes {
    type Output = CallInfo;
    fn index(&self, i: usize) -> &CallInfo {
        assert!(i < self.n);
        // SAFETY: in bounds (checked); a shared reborrow of the raw allocation.
        unsafe { &*self.p.add(i) }
    }
}

impl Drop for Nodes {
    fn drop(&mut self) {
        // SAFETY: `p`/`n` came from `Box::into_raw` of a boxed slice of `n` elements.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(self.p, self.n)) });
    }
}

struct World {
    stack: Vec<u64>,
    cis: Nodes,
    th_ci: *mut CallInfo,
    th_top: usize,
    bud: Box<Bud>,
    /// `thread.hookmask != 0`: rebuilt frames must trap.
    hooked: bool,
    codes: Vec<Vec<u32>>,
    ks: Vec<Vec<u64>>,
}

impl World {
    fn new(nodes: usize) -> World {
        let cis = Nodes::new(nodes);
        let p = cis.as_mut_ptr();
        for i in 0..nodes {
            // SAFETY: `i < nodes`; the neighbours are inside the same boxed slice.
            unsafe {
                let c = &mut *p.add(i);
                c.previous = if i > 0 {
                    p.add(i - 1)
                } else {
                    std::ptr::null_mut()
                };
                c.next = if i + 1 < nodes {
                    p.add(i + 1)
                } else {
                    std::ptr::null_mut()
                };
                if i > 0 {
                    // a spare node: stale contents the restore must overwrite
                    c.func = StkId(0x5555);
                    c.top = StkId(0x5555);
                    c.savedpc = 0xBAD0 as *const u32;
                    c.kcache = 0xBAD8 as *const Value;
                    c.callstatus = 0xDEAD;
                    c.trap = 77;
                }
            }
        }
        // SAFETY: node 0 exists; plain field stores through the raw base pointer.
        unsafe {
            (*p).func = StkId(ROOT_FUNC);
            (*p).top = StkId(ROOT_FUNC + 1 + 12);
            (*p).callstatus = 0x1234;
            (*p).savedpc = 0xBAD0 as *const u32;
        }
        let th_ci = p;
        World {
            stack: vec![POISON; NSTACK * 2],
            cis,
            th_ci,
            th_top: 0x7777,
            bud: Bud::new(),
            hooked: false,
            codes: (0..9).map(|_| vec![0u32; 64]).collect(),
            ks: (0..9).map(|_| vec![0u64; 8]).collect(),
        }
    }

    fn ctx(&mut self) -> RestoreCtx {
        RestoreCtx {
            stack0: self.stack.as_mut_ptr().cast(),
            stack_size: NSTACK,
            root: self.cis.as_mut_ptr(),
            th_ci: &raw mut self.th_ci,
            th_top: &raw mut self.th_top,
            ncalls: std::ptr::null_mut(),
            budget: &raw mut self.bud.cnt,
            hooked: self.hooked,
        }
    }
}

struct Scn {
    snap: ExitSnap,
    fs: Vec<(u16, i16, u32, u16, i32)>, // func_slot, nresults, call_pc, maxstack, nextra (-1: not vararg) per frame
    pc: u32,
    refund: i32,
    vb_in: u64,
    vb: Loc,
    slots: Vec<(u16, u8, u64)>,
}

/// A random snapshot with `d` frames over `w` (code and constant arrays), its area contents and the expectations.
fn scenario(w: &mut World, d: usize, rng: &mut Rng, area: &mut ExitArea) -> Scn {
    let mut fs = Vec::new();
    let mut slot = 4 + rng.below(4) as u16;
    let mut frames = Vec::new();
    for k in 1..=d {
        slot += 3 + rng.below(6) as u16;
        let nres = rng.below(5) as i16 - 1;
        let call_pc = rng.below(40) as u32;
        let ms = 4 + rng.below(8) as u16;
        // P-E: about half of the frames are vararg frames (`nextraargs` is written, the node's other fields as before)
        let nx = if rng.chance(1, 2) { 1 + rng.below(6) as i32 } else { -1 };
        fs.push((slot, nres, call_pc, ms, nx));
        frames.push(FrameRestore {
            func_slot: slot,
            nresults: nres,
            call_pc,
            code: w.codes[k].as_ptr(),
            k: w.ks[k].as_ptr().cast(),
            maxstack: ms,
            tail: false,
            nextra: nx,
            mm: false,
        });
    }
    let pc = rng.below(50) as u32;
    // slots: distinct, in [0, 60), each with a random source kind
    let mut used = std::collections::BTreeSet::new();
    let mut slots = Vec::new();
    let mut entries = Vec::new();
    let (mut nreg, mut nxmm, mut nsp) = (0u8, 0u8, 0u16);
    for _ in 0..(rng.below(30) + 1) {
        let s = rng.below(60) as u16;
        if !used.insert(s) {
            continue;
        }
        let tag = SUPPORTED_TAGS[rng.below(SUPPORTED_TAGS.len() as u64) as usize];
        let val = rng.next();
        let src = match rng.below(4) {
            0 if nreg < 7 => {
                let r = [0u8, 1, 2, 3, 4, 5, 6, 8][usize::from(nreg)];
                nreg += 1;
                area.gpr[usize::from(r)] = val;
                ValSrc::Reg(r)
            }
            1 if nxmm < 15 => {
                area.xmm[usize::from(nxmm)] = val;
                nxmm += 1;
                ValSrc::Xmm(nxmm - 1)
            }
            2 => ValSrc::Imm(val),
            _ => {
                area.spill[usize::from(nsp)] = val;
                nsp += 1;
                ValSrc::Spill(nsp - 1)
            }
        };
        entries.push(SlotRestore {
            slot: s,
            tag,
            src,
            func: false,
        });
        slots.push((s, tag, val));
    }
    // the budget register: every valid kind (`vb_valid`): a spill slot, a general register (V8 is never a slot source
    // above) or the budget word itself. RA never places the budget in an X register.
    let vb_in = 100 + rng.below(1000);
    let vb = match rng.below(3) {
        0 => {
            area.spill[200] = vb_in;
            Loc::Spill(200)
        }
        1 => {
            area.gpr[8] = vb_in;
            Loc::G(8)
        }
        _ => {
            w.bud.cnt = vb_in as i32;
            Loc::None
        }
    };
    let refund = rng.below(200) as i32 - 100;
    let snap = ExitSnap {
        pc,
        slots: entries,
        frames,
        refund,
        land: if rng.chance(1, 2) {
            Land::Pre
        } else {
            Land::Mid
        },
        land_addr: 0x4000_0000 + u64::from(pc),
        usable: true,
        link_cell: 0,
        root_code: std::ptr::null(),
        seg_shift: 0,
        seg_pops: 0,
        sunk: Vec::new(),
        // F4: a consumer of a multiple-result sequence re-executes with the interpreter's `top` (`thread.top = base + slot`); the shipped
        // programs only reach such an exit through the generic handler, so the restore is checked here directly (mutants `exit_top_ignored`,
        // `exit_top_off_by_one_slot`: no program of tr_core exits at a snapshot with a top any more)
        top: if rng.chance(1, 3) { Some(rng.below(30) as u16) } else { None },
    };
    Scn {
        snap,
        fs,
        pc,
        refund,
        vb_in,
        vb,
        slots,
    }
}

fn check_scenario(
    w: &mut World,
    sc: &Scn,
    d: usize,
    before: &[u64],
    root_before: (usize, usize, u32),
    land: u64,
) {
    let base = ROOT_FUNC + 1;
    assert_eq!(land, 0x4000_0000 + u64::from(sc.pc));
    // stack: exactly the snapshot slots changed
    let mut want = before.to_vec();
    for &(s, tag, val) in &sc.slots {
        let i = (base + usize::from(s)) * 2;
        want[i] = u64::from(tag);
        want[i + 1] = val;
    }
    assert_eq!(w.stack, want, "stack");
    // root frame: only savedpc (and the chain) is touched
    assert_eq!(
        (w.cis[0].func.0, w.cis[0].top.0, w.cis[0].callstatus),
        root_before,
        "root frame fields other than savedpc"
    );
    // frames
    let mut code_of = vec![w.codes[0].as_ptr()];
    for k in 1..=d {
        code_of.push(w.codes[k].as_ptr());
    }
    for k in 1..=d {
        let (fslot, nres, call_pc, ms, nx) = sc.fs[k - 1];
        let c = &w.cis[k];
        let func = base + usize::from(fslot);
        assert_eq!(c.func.0, func, "frame {k} func");
        assert_eq!(c.top.0, func + 1 + usize::from(ms), "frame {k} top");
        assert_eq!(
            c.callstatus,
            (i32::from(nres) + 1) as u32,
            "frame {k} callstatus (nresults + 1)"
        );
        assert_eq!(
            c.trap,
            i32::from(w.hooked),
            "frame {k} trap = (hookmask != 0)"
        );
        assert_eq!(c.kcache.cast::<u64>(), w.ks[k].as_ptr(), "frame {k} kcache");
        assert_eq!(c.nextraargs, nx.max(0), "frame {k} nextraargs (written for a vararg frame, left alone otherwise)");
        // the caller resumes after its CALL
        let caller = &w.cis[k - 1];
        assert_eq!(
            caller.savedpc,
            code_of[k - 1].wrapping_add(call_pc as usize + 1),
            "frame {k}: the caller's savedpc is the instruction after the CALL"
        );
    }
    let (inner_sp, inner_top) = (w.cis[d].savedpc, w.cis[d].top.0);
    assert_eq!(
        inner_sp,
        code_of[d].wrapping_add(sc.pc as usize),
        "innermost savedpc = pc"
    );
    assert_eq!(w.th_ci, w.cis.as_mut_ptr().wrapping_add(d), "thread.ci");
    let want_top = match sc.snap.top {
        Some(t) => base + usize::from(t),
        None => inner_top,
    };
    assert_eq!(
        w.th_top, want_top,
        "thread.top = ci.top of the innermost frame (or base + the snapshot's top slot)"
    );
    assert_eq!(
        w.bud.cnt,
        (sc.vb_in as u32).wrapping_add(sc.refund as u32) as i32,
        "budget word vb + refund"
    );
    // nodes beyond the innermost are untouched
    for c in w.cis.iter().skip(d + 1) {
        assert_eq!(
            (c.func.0, c.callstatus, c.trap),
            (0x5555, 0xDEAD, 77),
            "a spare node was touched"
        );
    }
}

fn restore_case(d: usize, seed: u64) {
    restore_case_h(d, seed, false);
}

fn restore_case_h(d: usize, seed: u64, hooked: bool) {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut w = World::new(7);
    w.hooked = hooked;
    let mut area = ExitArea::default();
    let sc = scenario(&mut w, d, &mut rng, &mut area);
    let before = w.stack.clone();
    let root_before = (w.cis[0].func.0, w.cis[0].top.0, w.cis[0].callstatus);
    let ctx = w.ctx();
    let root_code = w.codes[0].as_ptr();
    // SAFETY: the world is a consistent synthetic thread: a root frame with 6 spare nodes, a stack of NSTACK slots; the
    // code arrays are 64 long and `pc`, `call_pc + 1` are below 64.
    let land = unsafe { restore(&ctx, &sc.snap, root_code, sc.vb, &area) };
    check_scenario(&mut w, &sc, d, &before, root_before, land);
}

const SEEDS: u64 = if cfg!(miri) { 3 } else { 400 };

#[test]
fn restore_matches_the_model_for_0_to_4_frames() {
    for d in 0..=4 {
        for seed in 0..SEEDS {
            restore_case(d, seed + 1000 * d as u64);
        }
    }
}

#[test]
fn restore_hooked_thread_makes_rebuilt_frames_trap() {
    for d in 0..=4 {
        for seed in 0..SEEDS {
            restore_case_h(d, seed + 5000 * d as u64, true);
        }
    }
}

#[test]
#[should_panic(expected = "budget register location")]
fn restore_rejects_an_x_register_budget() {
    let mut rng = Rng(5);
    let mut w = World::new(3);
    let mut area = ExitArea::default();
    let sc = scenario(&mut w, 0, &mut rng, &mut area);
    let ctx = w.ctx();
    let rc = w.codes[0].as_ptr();
    // SAFETY: a consistent synthetic world; the call must panic before writing anything.
    unsafe { restore(&ctx, &sc.snap, rc, Loc::X(15), &area) };
}

#[test]
fn vb_validity_is_one_rule() {
    assert!(
        vb_valid(Loc::G(0))
            && vb_valid(Loc::G(8))
            && vb_valid(Loc::Spill(3))
            && vb_valid(Loc::None)
    );
    assert!(!vb_valid(Loc::G(7)) && !vb_valid(Loc::X(0)) && !vb_valid(Loc::Const(0)));
    for bad in [Loc::G(7), Loc::X(15)] {
        let r = std::panic::catch_unwind(|| {
            let s = ExitSnap {
                pc: 0,
                slots: vec![],
                frames: vec![],
                refund: 0,
                land: Land::Pre,
                land_addr: 0x1000,
                usable: true,
                link_cell: 0,
                root_code: std::ptr::null(),
                seg_shift: 0,
                seg_pops: 0,
        sunk: Vec::new(),
        top: None,
            };
            inline_stub_ops(&s, bad)
        });
        assert!(r.is_err(), "inline stub accepted {bad:?}");
    }
}

#[test]
fn restore_each_frame_count_individually() {
    // named per frame count, so a failure says which
    for d in 0..=4usize {
        restore_case(d, 77 + d as u64);
    }
}

#[test]
#[should_panic(expected = "snapshot slot beyond the stack")]
fn restore_rejects_a_slot_beyond_the_stack() {
    let mut w = World::new(3);
    let area = ExitArea::default();
    let snap = ExitSnap {
        pc: 0,
        slots: vec![SlotRestore {
            slot: 9999,
            tag: LUA_VNIL,
            src: ValSrc::Imm(0),
            func: false,
        }],
        frames: vec![],
        refund: 0,
        land: Land::Pre,
        land_addr: 1,
        usable: true,
        link_cell: 0,
        root_code: std::ptr::null(),
        seg_shift: 0,
        seg_pops: 0,
        sunk: Vec::new(),
        top: None,
    };
    let ctx = w.ctx();
    let rc = w.codes[0].as_ptr();
    // SAFETY: as in `restore_case`; the bad slot is rejected before any write.
    unsafe { restore(&ctx, &snap, rc, Loc::None, &area) };
}

#[test]
#[should_panic(expected = "no spare CallInfo")]
fn restore_without_spare_nodes_panics_instead_of_corrupting() {
    let mut w = World::new(2);
    let mut rng = Rng(5);
    let mut area = ExitArea::default();
    let sc = scenario(&mut w, 3, &mut rng, &mut area);
    let ctx = w.ctx();
    let rc = w.codes[0].as_ptr();
    // SAFETY: as in `restore_case`, except that the chain is too short; the restore checks that.
    unsafe { restore(&ctx, &sc.snap, rc, sc.vb, &area) };
}

// ---- 2. snapshots resolved from the IR and the allocation -------------------------------------------------------

struct FakeResolver {
    code: Vec<Vec<u32>>,
    k: Vec<Vec<u64>>,
}

impl Resolver for FakeResolver {
    fn proto(&self, key: ProtoKey) -> Option<ProtoInfoR> {
        let i = key.0;
        Some(ProtoInfoR {
            code: self.code.get(i)?.as_ptr(),
            k: self.k.get(i)?.as_ptr().cast(),
            maxstack: 6 + i as u16,
        })
    }
    fn land_addr(&self, key: ProtoKey, pc: u32, land: Land) -> Option<u64> {
        Some(
            0x1000_0000
                + (key.0 as u64) * 0x10000
                + u64::from(pc) * 4
                + u64::from(land == Land::Mid),
        )
    }
}

fn ins(op: TOp, t: Ty, flags: u8, a: Ref, b: Ref, aux: u32) -> Ins {
    Ins {
        op,
        t,
        flags,
        a,
        b,
        prev: NOREF,
        aux,
    }
}

/// N2a: a snapshot of segment 1 (after a `RETF`) lands in the segment's prototype and carries the segment's root code;
/// a segment-0 snapshot keeps the null code (the trace-wide `ExitMeta::root_code`). Detector of `seg_root_code_stale` /
/// `seg_land_proto_stale`.
#[test]
fn n2a_segment_snapshots_use_the_segments_prototype_and_code() {
    let mut ir = TraceIr::default();
    ir.head = (ProtoKey(0), 3);
    let g = F_GUARD;
    let s0 = ir.snapshot(Snap { pc: 3, ..Snap::default() }, &[]);
    ir.emit(ins(TOp::Sload, Ty::Int, g, 0, NOREF, guard_aux(s0, 1)));
    let r = |n: u16| REF_BIAS + n;
    ir.segs = vec![
        Seg { proto: ProtoKey(0), code: 0, shift: 0, pops: 0, call_pc: 0, a: 0, retf_ins: NOREF },
        Seg { proto: ProtoKey(1), code: 0xABC0, shift: 5, pops: 1, call_pc: 4, a: 4, retf_ins: NOREF },
    ];
    let ent = |slot, r| SnapEntry { slot, r, flags: 0 };
    // seg 1, frameless, pc 6 of proto 1
    let s1 = ir.snapshot(Snap { pc: 6, seg: 1, ..Snap::default() }, &[ent(2, r(0))]);
    // seg 1 with one inlined frame of proto 2
    let s2 = ir.snapshot(
        Snap { pc: 1, seg: 1, frame: 1, nframes: 1, ..Snap::default() },
        &[ent(7, r(0))],
    );
    ir.frames = vec![FrameLink {
        proto: ProtoKey(2),
        func_slot: 6,
        call_pc: 9,
        nresults: 1,
        tail: false,
        nextra: -1,
        mm: false,
    }];
    ir.emit(ins(TOp::Ne, Ty::Int, g, r(0), r(0), guard_aux(s1, 0)));
    ir.emit(ins(TOp::Lt, Ty::Int, g, r(0), r(0), guard_aux(s2, 0)));
    let al = ra::allocate(&ir, &[], &ra::RaOpts::default()).expect("allocates");
    let res = FakeResolver {
        code: (0..3).map(|_| vec![0u32; 32]).collect(),
        k: (0..3).map(|_| vec![0u64; 4]).collect(),
    };
    let (snaps, _) = build_exits(&ir, &al, &res).expect("builds");
    assert!(snaps[0].root_code.is_null());
    assert_eq!((snaps[0].seg_shift, snaps[0].seg_pops), (0, 0));
    assert_eq!(snaps[1].root_code as usize, 0xABC0, "segment root code");
    assert_eq!((snaps[1].seg_shift, snaps[1].seg_pops), (5, 1));
    assert_eq!(snaps[1].land_addr, 0x1000_0000 + 0x10000 + 6 * 4, "landing in the segment's prototype");
    assert_eq!(snaps[2].land_addr, 0x1000_0000 + 2 * 0x10000 + 4, "landing in the inlined frame's prototype");
    assert_eq!(snaps[2].root_code as usize, 0xABC0);
    // the meta-wide code is used only for segment 0
    let meta_root = 0x7700usize as *const u32;
    assert_eq!(snaps[0].code_or(meta_root), meta_root);
    assert_eq!(snaps[1].code_or(meta_root) as usize, 0xABC0, "segment code wins over the trace-wide one");
    // a snapshot of a segment that does not exist is refused
    let mut bad = ir.clone();
    bad.snaps[1].seg = 3;
    assert!(build_exits(&bad, &al, &res).is_err());
}

#[test]
fn build_resolves_slots_frames_and_landing_from_the_allocation() {
    // proto keys: 0 = root, 1 and 2 = callees
    let mut ir = TraceIr::default();
    let k7 = ir.konst(KVal::Int(7));
    let kf = ir.konst(KVal::Num(2.5f64.to_bits()));
    let kt = ir.konst(KVal::Pri(Ty::True));
    ir.head = (ProtoKey(0), 3);
    ir.extent = 40;
    ir.depth = 2;
    let r = |n: u16| REF_BIAS + n;
    // entry snapshot 0: empty (state at the head)
    let s_entry = ir.snapshot(
        Snap {
            pc: 3,
            ..Snap::default()
        },
        &[],
    );
    let g = F_GUARD;
    let list = [
        ins(TOp::Sload, Ty::Int, g, 0, NOREF, guard_aux(s_entry, 1)), // 0 int  slot 0
        ins(TOp::Sload, Ty::Num, g, 1, NOREF, guard_aux(s_entry, 1)), // 1 num  slot 1
        ins(TOp::Sload, Ty::Tab, g, 2, NOREF, guard_aux(s_entry, 1)), // 2 tab  slot 2
        ins(TOp::Sload, Ty::Lcf, g, 3, NOREF, guard_aux(s_entry, 1)), // 3 closure slot 3
        ins(TOp::Add, Ty::Int, 0, r(0), k7, 0),                       // 4
        ins(TOp::Fadd, Ty::Num, 0, r(1), kf, 0),                      // 5
    ];
    for i in list {
        ir.emit(i);
    }
    let e = |slot, r, flags| SnapEntry { slot, r, flags };
    // snapshot 1: frameless; snapshot 2: two frames (outer callee 1, inner callee 2), FUNC entries
    let s1 = ir.snapshot(
        Snap {
            pc: 9,
            frame: 0,
            nframes: 0,
            refund: 3,
            land: Land::Mid,
            ..Snap::default()
        },
        &[
            e(10, r(4), 0),
            e(11, r(5), 0),
            e(12, r(2), 0),
            e(13, k7, 0),
            e(14, kt, 0),
            e(15, kf, 0),
        ],
    );
    let s2 = ir.snapshot(
        Snap {
            pc: 5,
            frame: 2,
            nframes: 2,
            refund: -4,
            land: Land::Pre,
            ..Snap::default()
        },
        &[
            e(20, r(3), SNAP_FUNC),
            e(21, r(4), 0),
            e(22, r(3), SNAP_FUNC),
            e(23, r(5), 0),
        ],
    );
    ir.frames = vec![
        FrameLink {
            proto: ProtoKey(1),
            func_slot: 20,
            call_pc: 8,
            nresults: 1,
            tail: false,
            nextra: -1,
            mm: false,
        },
        FrameLink {
            proto: ProtoKey(2),
            func_slot: 22,
            call_pc: 2,
            nresults: 0,
            tail: false,
            nextra: -1,
            mm: false,
        },
    ];
    ir.emit(ins(TOp::Ne, Ty::Int, g, r(4), k7, guard_aux(s1, 0))); // 6
    ir.emit(ins(TOp::Lt, Ty::Int, g, r(4), r(0), guard_aux(s2, 0))); // 7
                                                                     // the Sloads of slots 0..3 are the entry values; their stack copies are untouched
    let al = ra::allocate(&ir, &[], &ra::RaOpts::default()).expect("allocates");
    let res = FakeResolver {
        code: (0..3).map(|_| vec![0u32; 32]).collect(),
        k: (0..3).map(|_| vec![0u64; 4]).collect(),
    };
    let (snaps, descs) = build_exits(&ir, &al, &res).expect("builds");
    assert_eq!(snaps.len(), 3);
    assert_eq!(descs.len(), 3);
    // sites: the entry snapshot is all typechecks; the others are branch guards
    assert_eq!(descs[0].site, rn_jit::tr::cache::ExitSite::TypeGuard);
    assert_eq!(descs[1].site, rn_jit::tr::cache::ExitSite::Other);
    // frameless snapshot
    let s = &snaps[1];
    assert_eq!(
        (s.pc, s.refund, s.land, s.frames.len()),
        (9, 3, Land::Mid, 0)
    );
    assert_eq!(s.land_addr, 0x1000_0000 + 9 * 4 + 1);
    let tags: Vec<(u16, u8)> = s.slots.iter().map(|x| (x.slot, x.tag)).collect();
    assert_eq!(
        tags,
        vec![
            (10, LUA_VNUMINT),
            (11, LUA_VNUMFLT),
            (12, LUA_VTABLE | (1 << 6)),
            (13, LUA_VNUMINT),
            (14, LUA_VTRUE),
            (15, LUA_VNUMFLT)
        ]
    );
    assert_eq!(s.slots[3].src, ValSrc::Imm(7));
    assert_eq!(s.slots[4].src, ValSrc::Imm(0));
    assert_eq!(s.slots[5].src, ValSrc::Imm(2.5f64.to_bits()));
    // framed snapshot
    let s = &snaps[2];
    assert_eq!((s.pc, s.refund, s.frames.len()), (5, -4, 2));
    assert_eq!(
        s.land_addr,
        0x1000_0000 + 2 * 0x10000 + 5 * 4,
        "landing label of the innermost proto"
    );
    assert_eq!(s.frames[0].func_slot, 20);
    assert_eq!(s.frames[1].call_pc, 2);
    assert_eq!(s.frames[1].maxstack, 8);
    assert!(s.slots[0].func && s.slots[2].func && !s.slots[1].func);
    assert_eq!(s.slots[0].tag, LUA_VLCL | (1 << 6));
    // restore it from a simulated register file and compare with the plain values
    let mut w = World::new(5);
    let mut area = ExitArea::default();
    let mut expect = Vec::new();
    for (en, &loc) in ir.snap_entries(2).iter().zip(&al.snap_locs[2]) {
        let v = 0xA000 + u64::from(en.slot);
        match loc {
            Loc::G(n) => area.gpr[usize::from(n)] = v,
            Loc::X(n) => area.xmm[usize::from(n)] = v,
            Loc::Spill(n) => area.spill[usize::from(n)] = v,
            other => panic!("{other:?}"),
        }
        expect.push((en.slot, loc));
    }
    // two entries share a ref (slots 20 and 22 hold the same closure): one location, both slots get its value
    assert_eq!(al.snap_locs[2][0], al.snap_locs[2][2]);
    let expect: Vec<(u16, u64)> = expect
        .into_iter()
        .map(|(slot, loc)| {
            let v = match loc {
                Loc::G(n) => area.gpr[usize::from(n)],
                Loc::X(n) => area.xmm[usize::from(n)],
                Loc::Spill(n) => area.spill[usize::from(n)],
                other => panic!("{other:?}"),
            };
            (slot, v)
        })
        .collect();
    let ctx = w.ctx();
    let rc = w.codes[0].as_ptr();
    // SAFETY: as in `restore_case`.
    let land = unsafe { restore(&ctx, &snaps[2], rc, Loc::None, &area) };
    assert_eq!(land, snaps[2].land_addr);
    for (slot, v) in expect {
        let i = (ROOT_FUNC + 1 + usize::from(slot)) * 2;
        assert_eq!(w.stack[i + 1], v, "slot {slot}");
    }
    assert_eq!(w.cis[1].callstatus, 2);
    assert_eq!(w.cis[2].callstatus, 1);
    assert_eq!(w.cis[0].savedpc, rc.wrapping_add(9));
}

#[test]
fn build_reports_inconsistent_input() {
    let mut ir = TraceIr {
        head: (ProtoKey(0), 0),
        ..TraceIr::default()
    };
    ir.snapshot(
        Snap {
            nframes: 1,
            frame: 1,
            ..Snap::default()
        },
        &[],
    );
    let al = ra::allocate(&ir, &[], &ra::RaOpts::default()).expect("allocates");
    let res = FakeResolver {
        code: vec![vec![0; 4]],
        k: vec![vec![0; 1]],
    };
    // the snapshot wants a frame link that does not exist
    assert!(build_exits(&ir, &al, &res).is_err());
}

#[test]
fn build_accepts_only_nil_false_true_as_primitive_constants() {
    for (t, ok) in [
        (Ty::Nil, true),
        (Ty::False, true),
        (Ty::True, true),
        (Ty::Tab, false),
        (Ty::ShrStr, false),
        (Ty::Lcf, false),
    ] {
        let mut ir = TraceIr {
            head: (ProtoKey(0), 0),
            ..TraceIr::default()
        };
        let k = ir.konst(KVal::Pri(t));
        ir.snapshot(
            Snap::default(),
            &[SnapEntry {
                slot: 1,
                r: k,
                flags: 0,
            }],
        );
        let al = ra::allocate(&ir, &[], &ra::RaOpts::default()).expect("allocates");
        let res = FakeResolver {
            code: vec![vec![0; 4]],
            k: vec![vec![0; 1]],
        };
        let r = build_exits(&ir, &al, &res);
        assert_eq!(r.is_ok(), ok, "{t:?}: {:?}", r.err());
        if ok {
            assert_eq!(r.unwrap().0[0].slots[0].src, ValSrc::Imm(0));
        }
    }
}

// ---- 3. twin tests against the real interpreter -----------------------------------------------------------------

fn init() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
            "the compiler must be installed by this test binary"
        );
        #[cfg(feature = "tr_mutants")]
        rn_jit::tr::mutants::apply_vm_mutants();
    });
}

#[derive(Clone, Debug, PartialEq)]
struct CiCap {
    func: usize,
    top: usize,
    savedpc_idx: isize,
    callstatus: u32,
    k_is_proto_k: bool,
    code: usize,
    k: usize,
    maxstack: u8,
}

#[derive(Clone, Debug, PartialEq)]
struct Cap {
    cis: Vec<CiCap>,
    slots: Vec<(u64, u64)>,
    base: usize,
    th_ci_idx: usize,
    th_top: usize,
}

/// What `cap` leaves behind: expected state, restored state, mapped slots, landing address.
type Captured = (Cap, Cap, Vec<u16>, u64);

thread_local! {
    static CAPS: RefCell<Option<Captured>> = const { RefCell::new(None) };
    static NOTES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// `cap` runs the real interpreter from the restored frames (the re-executed CALL of `cap` must return at once).
    static RESUMING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Line events seen since the cap point (hook tests).
    static EVENTS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// `(ACC, events)` of the run after the cap point (resume run, or the baseline run of mode 2).
    static FINAL: RefCell<Option<(String, u32)>> = const { RefCell::new(None) };
}

const MODE_RESTORE_AND_PUT_BACK: i64 = 0;
/// Leave the restored frames in place and let the interpreter finish the program from them.
const MODE_RESUME: i64 = 1;
/// Do nothing at `cap`; the interpreter finishes the program (the baseline of `MODE_RESUME`).
const MODE_BASELINE: i64 = 2;

fn line_hook(_st: &mut State, _event: i32, _line: i32) -> LuaResult<()> {
    EVENTS.with(|e| e.set(e.get() + 1));
    Ok(())
}

fn read_acc(st: &mut State) -> String {
    let g = st.globals();
    let v = st.raw_get_str(g, b"ACC").unwrap();
    v.as_string()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
        .unwrap_or_default()
}

/// `known`: the capture taken before the scrub. The function slots of the frames may be poisoned afterwards (unmapped), so
/// the prototype data then comes from it instead of the closure in the slot.
fn capture(st: &mut State, root: Ci, d: usize, end: usize, known: Option<&Cap>) -> Cap {
    let mut cis = Vec::new();
    let mut c = root;
    let base = root.r().func.0 + 1;
    for i in 0..=d {
        let r = c.r();
        let (code, k, maxstack) = match known {
            Some(kn) => (
                kn.cis[i].code as *const u32,
                kn.cis[i].k as *const Value,
                kn.cis[i].maxstack,
            ),
            None => {
                let cl = st.get(r.func).as_lclosure().expect("a Lua frame");
                let p = cl.proto();
                (p.code_ptr(), p.k_ptr(), p.maxstacksize)
            }
        };
        cis.push(CiCap {
            func: r.func.0,
            top: r.top.0,
            savedpc_idx: (r.savedpc as isize - code as isize) / 4,
            callstatus: r.callstatus,
            k_is_proto_k: r.kcache == k,
            code: code as usize,
            k: k as usize,
            maxstack,
        });
        if let Some(n) = c.next() {
            c = n;
        }
    }
    let mut slots = Vec::new();
    for i in base..end {
        let p = st.slot_ptr(StkId(i)).cast::<u64>();
        // SAFETY: `i` is below the thread's stack top (callers pass the native frame's `top`).
        slots.push(unsafe { (p.read(), p.add(1).read()) });
    }
    Cap {
        cis,
        slots,
        base,
        th_ci_idx: d,
        th_top: 0,
    }
}

/// `cap(d, seed, full)`: see the module docs. Runs inside the innermost call of the chain.
fn n_cap(st: &mut State) -> LuaResult<usize> {
    let d = st.arg(0).as_int().unwrap_or(0) as usize;
    let seed = st.arg(1).as_int().unwrap_or(1) as u64;
    let full = st.arg(2).as_int().unwrap_or(0) != 0;
    let mode = st.arg(3).as_int().unwrap_or(MODE_RESTORE_AND_PUT_BACK);
    if RESUMING.with(|r| r.get()) {
        return Ok(0); // the re-executed CALL of `cap` after the restore
    }
    if mode == MODE_BASELINE {
        EVENTS.with(|e| e.set(0));
        return Ok(0);
    }
    let hooked = st.hook_mask() != 0;
    let nat = st.ci();
    let inner = nat.previous().expect("the calling Lua frame");
    let mut root = inner;
    for _ in 0..d {
        root = root.previous().expect("the chain");
    }
    let end = nat.r().top.0.min(st.stack_size());
    let mut e = capture(st, root, d, end, None);
    let base = e.base;
    // expectations: the innermost frame is re-executed at the CALL of `cap` (one before the pc the interpreter saved),
    // thread.top = ci.top of the innermost frame
    e.cis[d].savedpc_idx -= 1;
    e.th_top = e.cis[d].top;
    // the snapshot, from the capture
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut area = ExitArea::default();
    let mut frames = Vec::new();
    for k in 1..=d {
        let c = &e.cis[k];
        frames.push(FrameRestore {
            func_slot: (c.func - base) as u16,
            nresults: (c.callstatus & 0xff) as i16 - 1,
            call_pc: (e.cis[k - 1].savedpc_idx - 1) as u32,
            code: c.code as *const u32,
            k: c.k as *const u8,
            maxstack: u16::from(c.maxstack),
            tail: false,
            nextra: -1,
            mm: false,
        });
    }
    let (mut nreg, mut nxmm, mut nsp) = (0usize, 0usize, 0u16);
    let mut entries = Vec::new();
    let mut mapped = Vec::new();
    for slot in 0..(end - base) {
        let (tagw, payload) = e.slots[slot];
        let tag = tagw as u8;
        if tagw != u64::from(tag) || !SUPPORTED_TAGS.contains(&tag) || (!full && rng.chance(2, 5)) {
            continue;
        }
        let src = match rng.below(4) {
            0 if nreg < 7 => {
                let r = [0u8, 1, 2, 3, 4, 5, 6, 8][nreg];
                nreg += 1;
                area.gpr[usize::from(r)] = payload;
                ValSrc::Reg(r)
            }
            1 if nxmm < 15 => {
                area.xmm[nxmm] = payload;
                nxmm += 1;
                ValSrc::Xmm((nxmm - 1) as u8)
            }
            2 => ValSrc::Imm(payload),
            _ => {
                area.spill[usize::from(nsp)] = payload;
                nsp += 1;
                ValSrc::Spill(nsp - 1)
            }
        };
        entries.push(SlotRestore {
            slot: slot as u16,
            tag,
            src,
            func: false,
        });
        mapped.push(slot as u16);
    }
    let pc = e.cis[d].savedpc_idx as u32;
    let refund = rng.below(40) as i32 - 20;
    let snap = ExitSnap {
        pc,
        slots: entries,
        frames,
        refund,
        land: Land::Mid,
        land_addr: 0x2000_0000 + u64::from(pc),
        usable: true,
        link_cell: 0,
        root_code: std::ptr::null(),
        seg_shift: 0,
        seg_pops: 0,
        sunk: Vec::new(),
        top: None,
    };
    // the budget register: every valid kind, chosen by the seed (G(8) is never a slot source above)
    let vb_kind = seed % 3;
    let vb = match vb_kind {
        0 => {
            area.gpr[8] = 5000;
            Loc::G(8)
        }
        1 => {
            area.spill[250] = 5000;
            Loc::Spill(250)
        }
        _ => Loc::None,
    };
    let mut bud = Bud::new();
    if vb_kind == 2 {
        bud.cnt = 5000;
    }
    // ---- back up everything the restore may touch
    let nodes: Vec<*mut CallInfo> = {
        let mut v = Vec::new();
        let mut c = root;
        for i in 0..=d + 1 {
            v.push(c.as_ptr());
            if i < d + 1 {
                c = c.next().expect("nodes up to the native frame");
            }
        }
        v
    };
    // SAFETY: the nodes are live list nodes; `CallInfo` is plain data, a bitwise copy is a valid backup.
    let backup: Vec<CallInfo> = nodes
        .iter()
        .map(|&p| unsafe { std::ptr::read(p) })
        .collect();
    let th_ci = st.ci();
    let th_top = st.top();
    let stack_backup: Vec<(u64, u64)> = e.slots.clone();
    // ---- scrub: the state a trace has at an exit (frames popped, nodes stale, snapshot slots stale)
    for (k, &p) in nodes.iter().enumerate().skip(1).take(d) {
        // SAFETY: live list nodes (k >= 1 are spare nodes of the root frame).
        unsafe {
            (*p).func = StkId(0x5555);
            (*p).top = StkId(0x5555);
            (*p).savedpc = (0xBAD0 + k) as *const u32;
            (*p).kcache = 0xBAD8 as *const Value;
            (*p).callstatus = 0xDEAD;
            (*p).trap = 77;
        }
    }
    root.w().savedpc = 0xBAD0 as *const u32;
    // every slot of the range is poisoned, mapped or not: a mapped one must be rewritten by the restore, an unmapped
    // one (a dead / popped-frame slot) must be left alone (the restore may not write anything the snapshot omits)
    for i in 0..(end - base) {
        let p = st.slot_ptr(StkId(base + i)).cast::<u64>();
        // SAFETY: inside the stack (below `end`).
        unsafe {
            p.write(POISON);
            p.add(1).write(POISON);
        }
    }
    st.set_ci(root);
    // ---- restore
    let th8 = rn_core::jit_hooks::thread_ptr(st);
    let ctx = RestoreCtx {
        stack0: st.slot_ptr(StkId(0)).cast(),
        stack_size: st.stack_size(),
        root: root.as_ptr(),
        th_ci: th8.wrapping_add(rn_core::jit_hooks::thread::CI).cast(),
        th_top: th8.wrapping_add(rn_core::jit_hooks::thread::TOP).cast(),
        ncalls: std::ptr::null_mut(),
        budget: &raw mut bud.cnt,
        hooked,
    };
    let root_code = e.cis[0].code as *const u32;
    // SAFETY: `ctx` describes the live thread; the root frame has the `d` spare nodes after it (they are the nodes of
    // the call chain just popped); the code pointers come from the live prototypes of the captured frames.
    let land = unsafe { restore(&ctx, &snap, root_code, vb, &area) };
    // ---- capture the result
    let mut r = capture(st, root, d, end, Some(&e));
    let cur = st.ci();
    let mut idx = 0;
    let mut c = root;
    while c.as_ptr() != cur.as_ptr() && idx < 16 {
        c = c.next().expect("chain");
        idx += 1;
    }
    r.th_ci_idx = idx;
    r.th_top = st.top().0;
    let trap_ok = (1..=d).all(|k| {
        // SAFETY: live list nodes.
        unsafe { (*nodes[k]).trap == i32::from(hooked) }
    });
    NOTES.with(|n| {
        let mut n = n.borrow_mut();
        if !trap_ok {
            n.push(format!(
                "trap of a rebuilt frame is not {}",
                i32::from(hooked)
            ));
        }
        if land != snap.land_addr {
            n.push(format!("landing {land:#x}"));
        }
        if bud.cnt != (5000 + refund) {
            n.push(format!("budget {} want {}", bud.cnt, 5000 + refund));
        }
    });
    if mode == MODE_RESUME {
        // leave the restored state in place: the real interpreter resumes the innermost frame at its `savedpc` (the
        // CALL of `cap`, which returns at once now) and returns through every rebuilt frame up to the root, whose
        // CIST_FRESH ends the nested run. Everything it did is compared with the baseline run by the test.
        CAPS.with(|c| *c.borrow_mut() = Some((e, r, mapped, land)));
        RESUMING.with(|x| x.set(true));
        EVENTS.with(|x| x.set(0));
        let inner = st.ci();
        let ex = st.g().executor;
        let res = ex(st, inner);
        RESUMING.with(|x| x.set(false));
        let acc = if res.is_ok() {
            read_acc(st)
        } else {
            "ERROR".into()
        };
        FINAL.with(|f| *f.borrow_mut() = Some((acc, EVENTS.with(|x| x.get()))));
        // unwind the (now finished) outer interpreter through the protected call of `exec`
        st.set_ci(nat);
        st.set_top(nat.r().top);
        return st.rt_error(b"resumed");
    }
    // ---- put everything back
    for (&p, b) in nodes.iter().zip(&backup) {
        // SAFETY: restoring the bitwise copy taken above into the same live node.
        unsafe { std::ptr::write(p, std::ptr::read(b)) };
    }
    for (i, &(a, b)) in stack_backup.iter().enumerate() {
        let p = st.slot_ptr(StkId(base + i)).cast::<u64>();
        // SAFETY: inside the stack.
        unsafe {
            p.write(a);
            p.add(1).write(b);
        }
    }
    st.set_ci(th_ci);
    st.set_top(th_top);
    CAPS.with(|c| *c.borrow_mut() = Some((e, r, mapped, land)));
    Ok(0)
}

fn new_state() -> LuaState {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Off);
    let g = st.globals();
    st.raw_set_str(g, b"cap", Value::native(n_cap)).unwrap();
    st
}

fn exec(st: &mut LuaState, src: &str) -> String {
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=snippet", None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = if status == Status::Ok {
        match (v.as_string(), v.as_number()) {
            (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            (None, Some(n)) => {
                String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned()
            }
            _ => "nil".to_string(),
        }
    } else {
        format!("ERROR {status:?}")
    };
    st.set_top(base);
    out
}

/// A chain of `d` Lua calls below `root`, with popped frames before it; `forms` picks the call form per level.
fn chain_program(d: usize, forms: u64, seed: u64, full: bool) -> String {
    chain_program_ex(d, forms, seed, full, MODE_RESTORE_AND_PUT_BACK, false)
}

/// `mode`: the fourth argument of `cap`; `coro`: run the whole chain inside a coroutine (the exit is then taken in a
/// coroutine's frame chain, on the coroutine's own `CallInfo` list and stack). Every level and the root append what they
/// computed after the `cap` call to the global `ACC`, so a resumed run is observable.
fn chain_program_ex(d: usize, forms: u64, seed: u64, full: bool, mode: i64, coro: bool) -> String {
    let mut s = String::new();
    s.push_str(
        "function pop(a) local b,c,e,f,g,h,i,j,k,l = a, a+0.5, 'str', {}, nil, true, false, a*2, print, {1} return b end\n\
         function pop2(a) local t = {a, 'x'} local u = t[1] * 1.5 local v = tostring(u) return v end\n\
         function pop3() local t0,t1,t2,t3,t4,t5,t6,t7,t8,t9,t10,t11,t12,t13,t14,t15,t16,t17,t18,t19,t20,t21,t22 = 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0 local big = 987654321 return big end\n",
    );
    let call = |k: usize, args: &str| -> String {
        match (forms >> (2 * k)) % 3 {
            0 => format!("local r = f{}({args})", k + 1),
            1 => format!("f{}({args})", k + 1),
            _ => format!("local r, q = f{}({args})", k + 1),
        }
    };
    let cap = format!("cap({d}, {seed}, {}, {mode})", i32::from(full));
    for k in 1..=d {
        let body = if k == d { cap.clone() } else { call(k, "u, v") };
        s.push_str(&format!(
            "function f{k}(a, b)\n local u = a + {k}\n local v = (b or 0) * 2.5\n local w = {{u, v}}\n local s = 'lvl{k}'\n {body}\n ACC = ACC .. '|' .. tostring(u) .. s .. tostring(v) .. tostring(w[1] + w[2])\n return u\nend\n"
        ));
    }
    let root_body = if d == 0 { cap } else { call(0, "x, 1.5") };
    let start = if coro {
        "return (coroutine.wrap(function() local r = root(7) return r end)())"
    } else {
        "return root(7)"
    };
    s.push_str(&format!(
        "ACC = ''\nfunction root(n)\n pop3()\n pop(n)\n pop2(n)\n local x = n * 3\n {root_body}\n ACC = ACC .. '|root' .. tostring(x)\n return x\nend\n{start}"
    ));
    s
}

fn twin_case(d: usize, seed: u64, full: bool) {
    twin_case_ex(d, seed, full, MODE_RESTORE_AND_PUT_BACK, false, false);
}

/// The twin check; `mode` as in `cap`; with `hook` a line hook is installed (the thread's `hookmask != 0`).
fn twin_case_ex(d: usize, seed: u64, full: bool, mode: i64, coro: bool, hook: bool) {
    let mut st = new_state();
    if hook {
        st.set_hook(Some(line_hook), rn_core::state::LUA_MASKLINE, 0);
    }
    CAPS.with(|c| *c.borrow_mut() = None);
    NOTES.with(|n| n.borrow_mut().clear());
    FINAL.with(|f| *f.borrow_mut() = None);
    let prog = chain_program_ex(d, seed.wrapping_mul(0x9E37), seed, full, mode, coro);
    let out = exec(&mut st, &prog);
    if mode == MODE_RESUME {
        assert!(
            out.starts_with("ERROR"),
            "{out} (the nested run ends in a raised error): {prog}"
        );
    } else {
        assert_eq!(out, "21", "the program itself ran: {prog}");
    }
    let (e, r, mapped, _land) = CAPS
        .with(|c| c.borrow_mut().take())
        .expect("cap was reached");
    let notes = NOTES.with(|n| n.borrow().clone());
    assert!(notes.is_empty(), "d={d} seed={seed}: {notes:?}");
    assert!(!mapped.is_empty());
    // frames
    assert_eq!(e.cis.len(), d + 1);
    for k in 0..=d {
        let (ec, rc) = (&e.cis[k], &r.cis[k]);
        assert_eq!(rc.func, ec.func, "d={d} seed={seed}: frame {k} func");
        assert_eq!(rc.top, ec.top, "d={d} seed={seed}: frame {k} top");
        assert_eq!(
            rc.savedpc_idx, ec.savedpc_idx,
            "d={d} seed={seed}: frame {k} savedpc (the caller resumes after its CALL)"
        );
        assert_eq!(
            rc.callstatus, ec.callstatus,
            "d={d} seed={seed}: frame {k} callstatus (nresults + 1)"
        );
        if k >= 1 {
            assert!(rc.k_is_proto_k, "d={d} seed={seed}: frame {k} kcache");
        }
    }
    assert_eq!(r.th_ci_idx, d, "thread.ci is the innermost inlined frame");
    assert_eq!(
        r.th_top, e.th_top,
        "thread.top = ci.top of the innermost frame"
    );
    // the whole stack range: the restored snapshot slots (popped frames' included) hold what the interpreter left,
    // and every slot the snapshot omits is still poison (the restore wrote nothing it was not told to)
    assert_eq!(e.slots.len(), r.slots.len());
    for (i, (a, b)) in e.slots.iter().zip(&r.slots).enumerate() {
        if mapped.contains(&(i as u16)) {
            assert_eq!(a, b, "d={d} seed={seed}: stack slot {i} (root-relative)");
        } else {
            assert_eq!(
                (b.0, b.1),
                (POISON, POISON),
                "d={d} seed={seed}: unmapped slot {i} was written"
            );
        }
    }
    if mode == MODE_RESUME {
        // the interpreter resumed from the restored frames and returned through every one of them: same observable
        // result as the interpreter that never saw a trace
        let (acc, events) = FINAL
            .with(|f| f.borrow_mut().take())
            .expect("the nested run finished");
        let mut st2 = new_state();
        if hook {
            st2.set_hook(Some(line_hook), rn_core::state::LUA_MASKLINE, 0);
        }
        let base_prog = chain_program_ex(
            d,
            seed.wrapping_mul(0x9E37),
            seed,
            full,
            MODE_BASELINE,
            coro,
        );
        assert_eq!(exec(&mut st2, &base_prog), "21");
        let base_acc = read_acc(&mut st2);
        let base_events = EVENTS.with(|x| x.get());
        assert_eq!(
            acc, base_acc,
            "d={d} seed={seed}: ACC after the resumed run vs the interpreter alone"
        );
        assert!(acc.ends_with("|root21"), "{acc}");
        assert_eq!(
            acc.matches('|').count(),
            d + 1,
            "{acc}: one record per returned frame"
        );
        if hook {
            // every rebuilt frame traps: the callers' lines after the call are seen by the hook (the baseline sees
            // them too; the re-executed CALL itself may or may not count as a new line)
            assert!(
                events + 1 >= base_events && events >= d as u32,
                "d={d} seed={seed}: events {events} vs baseline {base_events}"
            );
        }
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs rn-std (extern static stdin) and executable memory"
)]
fn twin_restore_equals_the_interpreter_for_0_to_4_frames() {
    for d in 0..=4 {
        twin_case(d, 1 + d as u64, true);
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs rn-std (extern static stdin) and executable memory"
)]
fn twin_restore_with_random_sparse_snapshots_and_call_forms() {
    let n = if cfg!(miri) { 2 } else { 60 };
    for d in 0..=4usize {
        for seed in 0..n {
            twin_case(d, 100 * d as u64 + seed + 7, false);
        }
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs rn-std (extern static stdin) and executable memory"
)]
fn twin_resume_from_the_restored_frames_equals_the_interpreter_alone() {
    // full maps (every live slot restored, every other one poisoned), all budget-register kinds (seed % 3)
    for d in 0..=4usize {
        for seed in 0..9u64 {
            twin_case_ex(
                d,
                3000 + 10 * d as u64 + seed,
                true,
                MODE_RESUME,
                false,
                false,
            );
        }
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs rn-std (extern static stdin) and executable memory"
)]
fn twin_hooked_thread_rebuilt_frames_trap_and_resume() {
    for d in 0..=4usize {
        for seed in 0..3u64 {
            twin_case_ex(
                d,
                4000 + 10 * d as u64 + seed,
                true,
                MODE_RESTORE_AND_PUT_BACK,
                false,
                true,
            );
            twin_case_ex(
                d,
                4100 + 10 * d as u64 + seed,
                true,
                MODE_RESUME,
                false,
                true,
            );
        }
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs rn-std (extern static stdin) and executable memory"
)]
fn twin_exit_inside_a_coroutine_frame_chain() {
    for d in 0..=4usize {
        for seed in 0..6u64 {
            twin_case_ex(
                d,
                5000 + 10 * d as u64 + seed,
                seed % 2 == 0,
                MODE_RESTORE_AND_PUT_BACK,
                true,
                false,
            );
        }
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs rn-std (extern static stdin) and executable memory"
)]
fn twin_popped_frame_slots_are_part_of_the_snapshot() {
    // `pop3` ran and returned: its 24th register (a distinctive integer) is left above the live frames. With a full map it
    // is restored like any other slot (TR3: popped-frame slots are never dropped).
    let mut st = new_state();
    CAPS.with(|c| *c.borrow_mut() = None);
    NOTES.with(|n| n.borrow_mut().clear());
    assert_eq!(exec(&mut st, &chain_program(1, 0, 5, true)), "21");
    let (e, r, mapped, _) = CAPS.with(|c| c.borrow_mut().take()).unwrap();
    let live_top = e.cis[1].top - e.base;
    let at = e
        .slots
        .iter()
        .position(|&(t, p)| t == u64::from(LUA_VNUMINT) && p == 987_654_321)
        .expect("the popped frame left its value in the stack");
    assert!(
        at >= live_top,
        "slot {at} is above the live frames (top {live_top})"
    );
    assert!(mapped.contains(&(at as u16)));
    assert_eq!(r.slots[at], e.slots[at]);
    assert!(NOTES.with(|n| n.borrow().is_empty()));
}

// ---- 4. the shared exit routine and the P0 slice in the emulator ------------------------------------------------

mod emu_support {
    use super::*;
    use rn_jit::emu::{Env, HelperOut, Helpers};
    pub struct MetaHelpers<'a> {
        pub meta: &'a ExitMeta,
        pub stack0_of: &'a dyn Fn(*mut u8) -> *mut u8,
        pub stack_size: usize,
        pub calls: u32,
        pub out: Option<CoreOut>,
        /// `(thread.ci, thread.top)` right after the handler.
        pub th_after: Option<(usize, usize)>,
    }

    impl Helpers for MetaHelpers<'_> {
        unsafe fn call(
            &mut self,
            _f: u64,
            _a: u32,
            _b: u32,
            env: &Env,
            frame: *mut u8,
        ) -> HelperOut {
            self.calls += 1;
            // SAFETY: `env.cnt` is the budget word of a `Bud` (the area is `FROM_R15` bytes behind it).
            let area = unsafe {
                &mut *env
                    .cnt
                    .cast::<u8>()
                    .add(exit_area::FROM_R15 as usize)
                    .cast::<ExitArea>()
            };
            let ctx = RestoreCtx {
                stack0: (self.stack0_of)(frame),
                stack_size: self.stack_size,
                root: env.ci.cast(),
                th_ci: env
                    .thread
                    .wrapping_add(rn_core::jit_hooks::thread::CI)
                    .cast(),
                th_top: env
                    .thread
                    .wrapping_add(rn_core::jit_hooks::thread::TOP)
                    .cast(),
                budget: env.cnt,
                ncalls: std::ptr::null_mut(),
                hooked: false,
            };
            // SAFETY: the synthetic thread of the test is consistent (see the callers).
            self.out = Some(unsafe { exit_core(self.meta, &ctx, area) });
            // SAFETY: the same synthetic thread words the restore just wrote.
            self.th_after = Some(unsafe { (ctx.th_ci.read() as usize, ctx.th_top.read()) });
            HelperOut {
                result: 0,
                new_frame: None,
            }
        }
    }
}

#[test]
fn shared_exit_routine_saves_registers_and_jumps_through_the_landing_cell() {
    use rn_jit::emu::{Emu, Env, HelperOut, Helpers, Stop, Until};
    use rn_jit::ir::{Op, Program};

    struct H {
        seen: Option<(u32, u32, [u64; 11], [u64; 16])>,
        new_ci: usize,
    }
    impl Helpers for H {
        unsafe fn call(&mut self, f: u64, a: u32, b: u32, env: &Env, _frame: *mut u8) -> HelperOut {
            assert_eq!((f, a, b), (0xABCD, 0, 0));
            // SAFETY: `env.cnt` is the budget word of a `Bud`.
            let area = unsafe { &mut *env.cnt.cast::<u8>().add(8).cast::<ExitArea>() };
            self.seen = Some((area.trace_id, area.exitno, area.gpr, area.xmm));
            area.args[0] = 0x1234_5000;
            // the handler "restored" a different frame: thread.ci changes
            // SAFETY: `env.thread` is a buffer large enough for the `ci` field.
            unsafe {
                env.thread
                    .add(rn_core::jit_hooks::thread::CI)
                    .cast::<usize>()
                    .write_unaligned(self.new_ci)
            };
            HelperOut {
                result: 0,
                new_frame: None,
            }
        }
    }
    let mut p = Program::default();
    let shared = append_shared_exit(
        &mut p,
        &SharedExitEnv {
            handler: 0xABCD,
            stack_off: 0,
        },
    );
    // op order of the routine: the handler may move the stack, so `ReloadFrame` must come after `SetCi` (and `SetK`
    // reads the `kcache` of the frame `SetCi` installed) and before anything that uses the frame register
    {
        let pos = |f: &dyn Fn(&Op) -> bool| p.ops.iter().position(f).expect("op present");
        let call = pos(&|o| matches!(o, Op::CallH { .. }));
        let set_ci = pos(&|o| matches!(o, Op::SetCi(_)));
        let reload = pos(&|o| matches!(o, Op::ReloadFrame { .. }));
        let set_k = pos(&|o| matches!(o, Op::SetK(_)));
        let jmp = pos(&|o| matches!(o, Op::JmpR(_)));
        assert!(
            call < set_ci && set_ci < reload && reload < set_k && set_k < jmp,
            "order CallH < SetCi < ReloadFrame < SetK < JmpR: {call} {set_ci} {reload} {set_k} {jmp}"
        );
        assert_eq!(
            p.ops
                .iter()
                .filter(|o| matches!(o, Op::ReloadFrame { .. }))
                .count(),
            1
        );
    }
    // the trace side: a generic stub for exit 5 of trace 9 after loading distinctive register values
    let entry = p.label();
    p.push(Op::Bind(entry));
    for v in 0..11u8 {
        p.push(Op::Mov(v, 0x1000 + u64::from(v)));
    }
    for x in 0..16u8 {
        p.push(Op::Mov(0, 0x2000 + u64::from(x)));
        p.push(Op::GtoF(x, 0));
    }
    p.push(Op::Mov(0, 0x1000));
    // the shared routine's address is mapped to its label below
    for op in generic_stub_ops(9, 5, 0xFEED_0000) {
        p.push(op);
    }
    let mut bud = Bud::new();
    let mut thread = vec![0u64; 1024];
    let mut ci_a = vec![0u64; 64];
    let mut ci_b = vec![0u64; 64];
    // ci_b.kcache
    // SAFETY: inside the buffer.
    unsafe {
        ci_b.as_mut_ptr()
            .cast::<u8>()
            .add(rn_core::jit_layout::callinfo::KCACHE)
            .cast::<usize>()
            .write(0x7777_0000)
    };
    let mut frame = vec![0u64; 64];
    let k = [0u64; 4];
    let env = Env {
        frame: frame.as_mut_ptr().cast(),
        ci: ci_a.as_mut_ptr().cast(),
        thread: thread.as_mut_ptr().cast(),
        k: k.as_ptr().cast(),
        cnt: (&raw mut *bud).cast::<i32>(),
    };
    let mut emu = Emu::new(&p, entry);
    emu.map_target(0xFEED_0000, shared);
    let mut h = H {
        seen: None,
        new_ci: ci_b.as_mut_ptr() as usize,
    };
    // SAFETY: the buffers are large enough for every offset used; the helper obeys the contract.
    let u = unsafe { emu.run(&p, &[], &env, 10_000, &mut h) };
    assert!(matches!(u, Until::Stop(Stop::Unsupported)), "{u:?}");
    assert_eq!(
        emu.jmp_target(),
        Some(0x1234_5000),
        "jumps to the landing address left in ExitArea.args[0]"
    );
    let (tid, exitno, gpr, xmm) = h.seen.expect("handler called");
    assert_eq!((tid, exitno), (9, 5));
    for (v, &g) in gpr.iter().enumerate() {
        if v != 7 {
            assert_eq!(g, 0x1000 + v as u64, "V{v} saved");
        }
    }
    for (x, &g) in xmm.iter().enumerate() {
        assert_eq!(g, 0x2000 + x as u64, "X{x} saved");
    }
    assert_eq!(
        emu.ci(&env),
        ci_b.as_mut_ptr().cast(),
        "SetCi from thread.ci"
    );
    assert_eq!(emu.k(&env) as usize, 0x7777_0000, "SetK from ci.kcache");
    // and it assembles
    #[cfg(not(miri))]
    rn_jit::x86::emit(&p).expect("assembles");
}

/// The inline stub (machine code, in the emulator) and `restore` agree on every byte for each budget-register kind
/// (G / Spill / None) and for the full 8-byte tag word: both start from the same poisoned frame and register file.
#[test]
fn inline_stub_equals_restore_for_every_budget_kind() {
    use rn_jit::emu::{Emu, Env, Stop, Until};
    use rn_jit::ir::Program;
    struct NoH;
    impl rn_jit::emu::Helpers for NoH {
        fn supported(&self) -> bool {
            false
        }
        unsafe fn call(
            &mut self,
            _: u64,
            _: u32,
            _: u32,
            _: &Env,
            _: *mut u8,
        ) -> rn_jit::emu::HelperOut {
            unreachable!()
        }
    }
    let mut kinds = [0u32; 3];
    for seed in 0..(if cfg!(miri) { 6 } else { 300 }) {
        let mut rng = Rng(seed * 0x9e37_79b9 + 1);
        let mut w = World::new(3);
        let mut area = ExitArea::default();
        let sc = scenario(&mut w, 0, &mut rng, &mut area);
        kinds[match sc.vb {
            Loc::G(_) => 0,
            Loc::Spill(_) => 1,
            _ => 2,
        }] += 1;
        // machine-code side: a copy of the world's stack and budget block
        let base = ROOT_FUNC + 1;
        let mut stack2 = w.stack.clone();
        let mut bud2 = Bud::new();
        bud2.cnt = w.bud.cnt;
        bud2.area = area.clone();
        // the stub keeps its register inputs in V-registers: the emulator starts them from the area
        let mut p = Program::default();
        let entry = p.label();
        p.push(rn_jit::ir::Op::Bind(entry));
        for r in 0..11u8 {
            if r != 7 {
                p.push(rn_jit::ir::Op::Mov(r, area.gpr[usize::from(r)]));
            }
        }
        for x in 0..16u8 {
            p.push(rn_jit::ir::Op::Mov(0, area.xmm[usize::from(x)]));
            p.push(rn_jit::ir::Op::GtoF(x, 0));
        }
        // V0 is clobbered by the loop above: reload it if it holds a value
        p.push(rn_jit::ir::Op::Mov(0, area.gpr[0]));
        for op in inline_stub_ops(&sc.snap, sc.vb) {
            p.push(op);
        }
        let mut thread = vec![0u64; 1024];
        let mut ci = vec![0u64; 64];
        let k = [0u64; 4];
        let env = Env {
            frame: stack2[base * 2..].as_mut_ptr().cast(),
            ci: ci.as_mut_ptr().cast(),
            thread: thread.as_mut_ptr().cast(),
            k: k.as_ptr().cast(),
            cnt: (&raw mut *bud2).cast::<i32>(),
        };
        let mut emu = Emu::new(&p, entry);
        // SAFETY: the buffers cover every offset the stub uses (slots < 60, the spill area inside `Bud`).
        let u = unsafe { emu.run(&p, &[], &env, 10_000, &mut NoH) };
        assert!(matches!(u, Until::Stop(Stop::Unsupported)), "{u:?}");
        assert_eq!(emu.jmp_target(), Some(sc.snap.land_addr));
        // restore side
        let before = w.stack.clone();
        let ctx = w.ctx();
        let rc = w.codes[0].as_ptr();
        // SAFETY: as in `restore_case`.
        unsafe { restore(&ctx, &sc.snap, rc, sc.vb, &area) };
        assert_eq!(
            stack2, w.stack,
            "seed {seed}: stub and restore write the same bytes (tag word included)"
        );
        assert_ne!(stack2, before);
        assert_eq!(
            bud2.cnt, w.bud.cnt,
            "seed {seed}: budget word, vb = {:?}",
            sc.vb
        );
        assert_eq!(
            bud2.cnt,
            (sc.vb_in as u32).wrapping_add(sc.refund as u32) as i32
        );
    }
    assert!(
        kinds.iter().all(|&n| n > 0),
        "every budget kind was exercised: {kinds:?}"
    );
}

/// N3b: the inline stub of a linking snapshot resumes the innermost frame at the same `savedpc` as the handler's restore and continues through the dispatch cell of the
/// link target (not at the tier-1 landing). Mutants `relink_savedpc_off`, `relink_jumps_to_land`.
#[test]
fn n3b_link_stub_inputs_equal_the_handlers() {
    use rn_jit::tr::exit::{frame_link_innermost_pc, frame_link_target, frame_mat_innermost_pc};
    for seed in 0..40u64 {
        for d in 1..=3usize {
            let mut rng = Rng(seed * 0x9e37_79b9 + d as u64 * 31 + 3);
            let mut w = World::new(7);
            let mut area = ExitArea::default();
            let mut sc = scenario(&mut w, d, &mut rng, &mut area);
            for f in &mut sc.snap.frames {
                f.nextra = -1;
            }
            sc.snap.link_cell = 0x1000 + seed * 8;
            sc.snap.land_addr = 0x2000 + seed * 16;
            let code = w.codes[0].as_ptr() as u64;
            assert_eq!(frame_link_innermost_pc(&sc.snap, code), frame_mat_innermost_pc(&sc.snap, code), "seed {seed} depth {d}");
            assert_eq!(frame_link_target(&sc.snap), (sc.snap.link_cell, true), "the stub jumps through the cell");
        }
    }
}

/// BUNDLE-H2: the machine-code frame rebuild of the inline stub (`frame_mat_ops`, run in the emulator) and `restore` (a hook-free thread) leave
/// every `CallInfo` field, `thread.ci` and `thread.top` the same, for 1..4 plain / tail frames. Detector of the `fmat_*` field mutants.
#[test]
#[cfg_attr(miri, ignore = "the emulator stores through addresses taken from the pieces of two worlds as integers; Miri reports a pointer without provenance (emu.rs store)")]
fn fmat_ops_equal_restore() {
    use rn_jit::emu::{Emu, Env, Stop, Until};
    use rn_jit::ir::{Op, Program};
    struct NoH;
    impl rn_jit::emu::Helpers for NoH {
        fn supported(&self) -> bool {
            false
        }
        unsafe fn call(&mut self, _: u64, _: u32, _: u32, _: &Env, _: *mut u8) -> rn_jit::emu::HelperOut {
            unreachable!()
        }
    }
    for seed in 0..(if cfg!(miri) { 4 } else { 200 }) {
        for d in 1..=4usize {
            let mut rng = Rng(seed * 0x9e37_79b9 + d as u64 * 77 + 5);
            let mut wa = World::new(7);
            let mut area = ExitArea::default();
            let mut sc = scenario(&mut wa, d, &mut rng, &mut area);
            for f in &mut sc.snap.frames {
                f.nextra = -1;
                f.tail = rng.chance(1, 3);
            }
            sc.snap.slots.clear();
            // (a snapshot with a multiple-result top is restored by the handler only: no inline stub, `frame_mat_ok`)
            sc.snap.top = None;
            // the second world: same codes / constants, same stale spare nodes
            let mut wb = World::new(7);
            wb.codes = wa.codes.clone();
            wb.ks = wa.ks.clone();
            wb.stack = wa.stack.clone();
            let root_code = wa.codes[0].as_ptr();
            let wa_ctx = wa.ctx();
            // SAFETY: as in `restore_case`.
            unsafe { restore(&wa_ctx, &sc.snap, root_code, Loc::None, &area) };
            // the machine-code side
            let mut p = Program::default();
            let entry = p.label();
            p.push(Op::Bind(entry));
            for op in frame_mat_ops(&sc.snap, root_code as u64) {
                p.push(op);
            }
            p.push(Op::Mov(7, sc.snap.land_addr));
            p.push(Op::JmpR(7));
            let mut thread = vec![0u64; 1024];
            let k = [0u64; 4];
            let mut bud = Bud::new();
            let mut stack = wb.stack.clone();
            let env = Env {
                frame: stack[(ROOT_FUNC + 1) * 2..].as_mut_ptr().cast(),
                ci: wb.cis.as_mut_ptr().cast(),
                thread: thread.as_mut_ptr().cast(),
                k: k.as_ptr().cast(),
                cnt: (&raw mut *bud).cast::<i32>(),
            };
            let mut emu = Emu::new(&p, entry);
            // SAFETY: the nodes, the thread buffer and the code arrays cover every offset the ops use.
            let u = unsafe { emu.run(&p, &[], &env, 10_000, &mut NoH) };
            assert!(matches!(u, Until::Stop(Stop::Unsupported)), "{u:?}");
            assert_eq!(emu.jmp_target(), Some(sc.snap.land_addr));
            // SAFETY: inside the thread buffer.
            let th_ci_b = unsafe { thread.as_ptr().cast::<u8>().add(rn_core::jit_hooks::thread::CI).cast::<usize>().read_unaligned() };
            // SAFETY: inside the thread buffer.
            let th_top_b = unsafe { thread.as_ptr().cast::<u8>().add(rn_core::jit_hooks::thread::TOP).cast::<usize>().read_unaligned() };
            let tag = format!("seed {seed} d {d}");
            for i in 0..=d {
                let (a, b) = (&wa.cis[i], &wb.cis[i]);
                assert_eq!(
                    (a.func.0, a.top.0, a.callstatus, a.trap, a.kcache, a.savedpc),
                    (b.func.0, b.top.0, b.callstatus, b.trap, b.kcache, b.savedpc),
                    "{tag}: node {i}"
                );
            }
            for i in d + 1..7 {
                assert_eq!((wb.cis[i].func.0, wb.cis[i].callstatus, wb.cis[i].trap), (0x5555, 0xDEAD, 77), "{tag}: spare node {i} touched");
            }
            assert_eq!(th_ci_b, wb.cis.as_mut_ptr().wrapping_add(d) as usize, "{tag}: thread.ci");
            assert_eq!(wa.th_ci, wa.cis.as_mut_ptr().wrapping_add(d), "{tag}: restore's thread.ci");
            assert_eq!(th_top_b, wa.th_top, "{tag}: thread.top");
        }
    }
}

// ---- the P0 slice ----

mod slice {
    use super::emu_support::*;
    use super::*;
    use rn_core::instruction::Instruction;
    use rn_core::jit_layout::callinfo as ci;
    use rn_core::ver::active::OpCode;
    use rn_jit::emu::{Emu, Env, Stop, Until};
    use rn_jit::ir::{Base, Cc, Op, Program, Src, L, M, V, W};
    use rn_jit::lower::{lower, LabelKind, LowerCfg, Lowered};
    use std::collections::HashMap;

    const NSLOT: usize = 40;
    const PRE: usize = 2;
    pub const H: u32 = 1;
    pub const FORLOOP_PC: u32 = 3;
    const N_ITER: u32 = 2;
    const VB: V = 5;
    const TRACE_ID: u32 = 3;

    pub fn code() -> &'static [u32] {
        static C: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
        C.get_or_init(|| bytecode().iter().map(|i| i.0).collect())
    }

    pub fn bytecode() -> Vec<Instruction> {
        vec![
            Instruction::abx(OpCode::Forprep, 0, 2),
            Instruction::abck(OpCode::Add, 4, 4, CTL, 0),
            Instruction::abck(OpCode::Mmbin, 4, 2, 6, 0),
            Instruction::abx(OpCode::Forloop, 0, 3),
            Instruction::abck(OpCode::Return1, 4, 2, 0, 0),
            Instruction::abck(OpCode::Return0, 0, 1, 0, 0),
        ]
    }

    /// The numeric `for` registers are written with their 5.5 numbers in this file (0 = count / limit, 1 = step, 2 = index /
    /// control variable); `ph` maps them to the version's physical slots (5.4 TR30: index `R[A]`, count `R[A+1]`, step
    /// `R[A+2]`, and the visible control variable copy `R[A+3]`).
    const fn ph(l: u16) -> u16 {
        if cfg!(rustynail_lua54) {
            match l {
                0 => 1,
                1 => 2,
                2 => 0,
                o => o,
            }
        } else {
            l
        }
    }

    /// The register the loop body reads as the control variable (`ADD s, s, i`).
    const CTL: u32 = if cfg!(rustynail_lua54) { 3 } else { 2 };

    /// The snapshot entries of the loop state: count, index (+ the 5.4 copy) and the accumulator.
    fn loop_state(cnt: Ref, idx: Ref, acc: Ref) -> Vec<SnapEntry> {
        let e = |slot, r| SnapEntry { slot, r, flags: 0 };
        let mut v = vec![e(ph(0), cnt), e(ph(2), idx), e(4, acc)];
        if cfg!(rustynail_lua54) {
            v.push(e(3, idx));
        }
        v.sort_by_key(|x| x.slot);
        v
    }

    const fn r(n: u16) -> Ref {
        REF_BIAS + n
    }

    /// The IR of docs/JIT_TRACE_REF.md section 2 and its hand register map (a copy of `tests/tr_ref.rs`).
    pub fn slice_ir(low: &Lowered) -> (TraceIr, HashMap<Ref, V>) {
        let mut t = TraceIr::default();
        let k0 = t.konst(KVal::Int(0));
        let k1 = t.konst(KVal::Int(1));
        t.head = (ProtoKey(0), H);
        t.extent = 5;
        t.counts = vec![N_ITER, N_ITER];
        let g = F_GUARD;
        let list = [
            ins(
                TOp::Budget,
                Ty::I64,
                g,
                N_ITER as Ref,
                NOREF,
                guard_aux(0, 0),
            ),
            ins(TOp::Sload, Ty::Int, g, 4, NOREF, guard_aux(1, 1)),
            ins(TOp::Sload, Ty::Int, g, ph(2) as Ref, NOREF, guard_aux(1, 1)),
            ins(TOp::Sload, Ty::Int, g, ph(0) as Ref, NOREF, guard_aux(1, 1)),
            ins(
                TOp::Sload,
                Ty::Int,
                g | F_LOOPINV,
                ph(1),
                NOREF,
                guard_aux(1, 1),
            ),
            ins(TOp::Add, Ty::Int, 0, r(1), r(2), 0),
            ins(TOp::Ne, Ty::Int, g, r(3), k0, guard_aux(2, 0)),
            ins(TOp::Sub, Ty::Int, 0, r(3), k1, 0),
            ins(TOp::Add, Ty::Int, 0, r(2), r(4), 0),
            ins(TOp::Loop, Ty::Nil, 0, NOREF, NOREF, 0),
            ins(TOp::Phi, Ty::Int, F_PHI, r(5), r(15), 0),
            ins(TOp::Phi, Ty::Int, F_PHI, r(8), r(18), 0),
            ins(TOp::Phi, Ty::Int, F_PHI, r(7), r(17), 0),
            ins(TOp::Poll, Ty::Nil, g, NOREF, NOREF, guard_aux(3, 0)),
            ins(
                TOp::Budget,
                Ty::I64,
                g,
                N_ITER as Ref,
                NOREF,
                guard_aux(3, 0),
            ),
            ins(TOp::Add, Ty::Int, 0, r(10), r(11), 0),
            ins(TOp::Ne, Ty::Int, g, r(12), k0, guard_aux(4, 0)),
            ins(TOp::Sub, Ty::Int, 0, r(12), k1, 0),
            ins(TOp::Add, Ty::Int, 0, r(11), r(4), 0),
        ];
        for (n, i) in list.into_iter().enumerate() {
            assert_eq!(t.emit(i), r(n as u16));
        }
        t.loop_at = Some(r(9));
        // The count model (TR5.4): refund = n_charged - fetches(P) - land(P).
        let mid_refund = N_ITER as i32 - low.fetches(&[H]) as i32 - low.land(FORLOOP_PC) as i32;
        let e = |slot, r| SnapEntry { slot, r, flags: 0 };
        let snap = |pc, land, refund| Snap {
            pc,
            land,
            refund,
            ..Snap::default()
        };
        assert_eq!(t.snapshot(snap(H, Land::Pre, 0), &[]), 0);
        assert_eq!(t.snapshot(snap(H, Land::Pre, N_ITER as i32), &[]), 1);
        assert_eq!(
            t.snapshot(snap(FORLOOP_PC, Land::Mid, mid_refund), &[e(4, r(5))]),
            2
        );
        assert_eq!(
            t.snapshot(snap(H, Land::Pre, 0), &loop_state(r(12), r(11), r(10))),
            3
        );
        assert_eq!(
            t.snapshot(
                snap(FORLOOP_PC, Land::Mid, mid_refund),
                &loop_state(r(12), r(11), r(15))
            ),
            4
        );
        let regs: HashMap<Ref, V> = [
            (r(1), 1),
            (r(2), 2),
            (r(3), 3),
            (r(4), 4),
            (r(5), 1),
            (r(7), 3),
            (r(8), 2),
            (r(10), 1),
            (r(11), 2),
            (r(12), 3),
            (r(15), 1),
            (r(17), 3),
            (r(18), 2),
        ]
        .into_iter()
        .collect();
        (t, regs)
    }

    /// An `Allocation` carrying only what the exit builder reads: the hand register map as `snap_locs`.
    pub fn hand_allocation(t: &TraceIr, regs: &HashMap<Ref, V>) -> Allocation {
        Allocation {
            ins: vec![],
            fused: vec![],
            weak: vec![],
            snap_locs: (0..t.snaps.len())
                .map(|s| {
                    t.snap_entries(s as u32)
                        .iter()
                        .map(|e| Loc::G(regs[&e.r]))
                        .collect()
                })
                .collect(),
            forced_spills: vec![],
            entry_moves: vec![],
            entry_seq: vec![],
            backedge_moves: vec![],
            backedge_seq: vec![],
            backedge_after: 0,
            end_locs: vec![],
            calls: vec![],
            nspill: 0,
        }
    }

    pub struct Mem {
        pub frame: Vec<u64>,
        pub ci: Vec<u64>,
        pub thread: Vec<u64>,
        pub k: Vec<u64>,
        pub bud: Box<Bud>,
        pub irq: Vec<u8>,
    }

    impl Clone for Mem {
        fn clone(&self) -> Mem {
            Mem {
                frame: self.frame.clone(),
                ci: self.ci.clone(),
                thread: self.thread.clone(),
                k: self.k.clone(),
                bud: self.bud.clone(),
                irq: self.irq.clone(),
            }
        }
    }

    impl Mem {
        pub fn new() -> Mem {
            let mut m = Mem {
                frame: vec![0; (NSLOT + PRE) * 2],
                ci: vec![0; ci::SIZE.div_ceil(8)],
                thread: vec![0; 1024],
                k: vec![0; 8],
                bud: Bud::new(),
                irq: vec![0],
            };
            m.bud.cnt = i32::MAX;
            m
        }
        pub fn set_reg(&mut self, r: usize, tag: u8, payload: u64) {
            // logical (5.5) numbers of the `for` registers, see `ph`; the 5.4 visible control variable is a copy
            let phys = ph(r as u16) as usize;
            self.frame[(PRE + phys) * 2] = u64::from(tag);
            self.frame[(PRE + phys) * 2 + 1] = payload;
            if cfg!(rustynail_lua54) && r == 2 {
                self.frame[(PRE + 3) * 2] = u64::from(tag);
                self.frame[(PRE + 3) * 2 + 1] = payload;
            }
        }
        fn put<T: Copy>(buf: &mut [u64], off: usize, v: T) {
            assert!(off + std::mem::size_of::<T>() <= buf.len() * 8);
            // SAFETY: bounds checked above.
            unsafe {
                buf.as_mut_ptr()
                    .cast::<u8>()
                    .add(off)
                    .cast::<T>()
                    .write_unaligned(v)
            }
        }
        fn get<T: Copy>(buf: &[u64], off: usize) -> T {
            assert!(off + std::mem::size_of::<T>() <= buf.len() * 8);
            // SAFETY: bounds checked above.
            unsafe {
                buf.as_ptr()
                    .cast::<u8>()
                    .add(off)
                    .cast::<T>()
                    .read_unaligned()
            }
        }
        pub fn setup_ci(&mut self, callstatus: u32) {
            Self::put(&mut self.ci, ci::CALLSTATUS, callstatus);
            Self::put(&mut self.ci, ci::TRAP, 0i32);
            Self::put(&mut self.ci, ci::PREVIOUS, 0x7000usize);
            Self::put(&mut self.ci, ci::FUNC, 5usize);
            Self::put(&mut self.ci, ci::TOP, 5 + 1 + NSLOT - 4);
            Self::put(&mut self.ci, ci::KCACHE, self.k.as_ptr() as usize);
        }
        pub fn savedpc(&self) -> usize {
            Self::get(&self.ci, ci::SAVEDPC)
        }
        pub fn env(&mut self) -> Env {
            Env {
                frame: self.frame.as_mut_ptr().cast::<u8>().wrapping_add(PRE * 16),
                ci: self.ci.as_mut_ptr().cast(),
                thread: self.thread.as_mut_ptr().cast(),
                k: self.k.as_ptr().cast(),
                cnt: (&raw mut *self.bud).cast::<i32>(),
            }
        }
    }

    fn lowered(code_addr: usize, irq: *const u8) -> Lowered {
        let words: Vec<u32> = bytecode().iter().map(|i| i.0).collect();
        lower(
            &words,
            LowerCfg {
                calls: None,
                code_addr,
                interrupt_addr: irq as usize,
                consts: &[],
                t15_warmup: None,
            },
        )
        .expect("lowers")
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Stubs {
        Inline,
        Generic,
    }

    fn frame_m(slot: u16, off: i32) -> M {
        M {
            base: Base::Frame,
            off: i32::from(slot) * 16 + off,
        }
    }

    /// Appends the trace to `low.prog` with exits as inline or generic stubs; returns the entry label, the label of the
    /// shared routine (generic) and the `(address, label)` pairs of every landing address.
    pub fn lower_slice(
        low: &mut Lowered,
        t: &TraceIr,
        regs: &HashMap<Ref, V>,
        snaps: &[ExitSnap],
        stubs: Stubs,
        irq: usize,
    ) -> (L, Vec<(u64, L)>) {
        let entry = low.prog.label();
        let loop_l = low.prog.label();
        let reg = |x: Ref| regs[&x];
        let src = |x: Ref| -> Src {
            if TraceIr::is_const(x) {
                match t.kval(x) {
                    KVal::Int(v) => Src::Imm(v as i32),
                    k => panic!("{k:?}"),
                }
            } else {
                Src::V(reg(x))
            }
        };
        let mut exits: Vec<(u32, L)> = Vec::new();
        let mut exit_for = |p: &mut Program, snap: u32| -> L {
            if let Some(&(_, l)) = exits.iter().find(|&&(s, _)| s == snap) {
                return l;
            }
            let l = p.label();
            exits.push((snap, l));
            l
        };
        let pcl: Vec<Option<L>> = low.pc_labels.clone();
        let p = &mut low.prog;
        p.push(Op::Bind(entry));
        p.push(Op::Ld {
            dst: VB,
            w: W::B4,
            m: M {
                base: Base::Cnt,
                off: 0,
            },
        });
        for (n, i) in t.ins.iter().enumerate() {
            let me = r(n as u16);
            match i.op {
                TOp::Budget => {
                    let x = exit_for(p, aux_snap(i.aux));
                    p.push(Op::Br {
                        cc: Cc::Lt,
                        a: VB,
                        b: Src::Imm(i32::from(i.a)),
                        to: x,
                    });
                    p.push(Op::Sub(VB, Src::Imm(i32::from(i.a))));
                }
                TOp::Sload => {
                    if aux_bits(i.aux) & 1 != 0 {
                        let x = exit_for(p, aux_snap(i.aux));
                        p.push(Op::Ld {
                            dst: 0,
                            w: W::B1,
                            m: frame_m(i.a, 0),
                        });
                        p.push(Op::Br {
                            cc: Cc::Ne,
                            a: 0,
                            b: Src::Imm(i32::from(LUA_VNUMINT)),
                            to: x,
                        });
                    }
                    p.push(Op::Ld {
                        dst: reg(me),
                        w: W::B8,
                        m: frame_m(i.a, 8),
                    });
                }
                TOp::Add | TOp::Sub => {
                    let (d, a) = (reg(me), reg(i.a));
                    if d != a {
                        p.push(Op::MovR(d, a));
                    }
                    let b = src(i.b);
                    p.push(if i.op == TOp::Add {
                        Op::Add(d, b)
                    } else {
                        Op::Sub(d, b)
                    });
                }
                TOp::Ne => {
                    let x = exit_for(p, aux_snap(i.aux));
                    p.push(Op::Br {
                        cc: Cc::Eq,
                        a: reg(i.a),
                        b: src(i.b),
                        to: x,
                    });
                }
                TOp::Poll => {
                    let x = exit_for(p, aux_snap(i.aux));
                    p.push(Op::Ld {
                        dst: 0,
                        w: W::B4,
                        m: M {
                            base: Base::Ci,
                            off: ci::TRAP as i32,
                        },
                    });
                    p.push(Op::Br {
                        cc: Cc::Ne,
                        a: 0,
                        b: Src::Imm(0),
                        to: x,
                    });
                    p.push(Op::Mov(6, irq as u64));
                    p.push(Op::Ld {
                        dst: 0,
                        w: W::B1,
                        m: M {
                            base: Base::Reg(6),
                            off: 0,
                        },
                    });
                    p.push(Op::Br {
                        cc: Cc::Ne,
                        a: 0,
                        b: Src::Imm(0),
                        to: x,
                    });
                }
                TOp::Loop => p.push(Op::Bind(loop_l)),
                TOp::Phi => {}
                op => panic!("{op:?}"),
            }
        }
        p.push(Op::Jmp(loop_l));
        let mut targets: Vec<(u64, L)> = Vec::new();
        let mut shared = None;
        if stubs == Stubs::Generic {
            let l = append_shared_exit(
                p,
                &SharedExitEnv {
                    handler: 0xFACE,
                    stack_off: 0,
                },
            );
            targets.push((0xFEED_0000, l));
            shared = Some(0xFEED_0000u64);
        }
        for (snap, l) in exits {
            p.push(Op::Bind(l));
            let s = &snaps[snap as usize];
            match stubs {
                Stubs::Inline => {
                    for op in inline_stub_ops(s, Loc::G(VB)) {
                        p.push(op);
                    }
                }
                Stubs::Generic => {
                    for op in generic_stub_ops(TRACE_ID, snap, shared.unwrap()) {
                        p.push(op);
                    }
                }
            }
        }
        // landing addresses -> tier-1 labels
        for s in snaps {
            let pc = ((s.land_addr - 0x6000_0000) / 8) as usize;
            targets.push((s.land_addr, pcl[pc].expect("landing label")));
        }
        (entry, targets)
    }

    struct SliceResolver {
        kinds: Vec<LabelKind>,
        code: Vec<u32>,
    }
    impl Resolver for SliceResolver {
        fn proto(&self, _key: ProtoKey) -> Option<ProtoInfoR> {
            Some(ProtoInfoR {
                code: self.code.as_ptr(),
                k: std::ptr::null(),
                maxstack: 8,
            })
        }
        fn land_addr(&self, _key: ProtoKey, pc: u32, land: Land) -> Option<u64> {
            // the snapshot's recorded kind must be what the lowering reports
            let want = if land == Land::Pre {
                LabelKind::Head
            } else {
                LabelKind::Mid
            };
            assert_eq!(self.kinds[pc as usize], want, "landing kind of pc {pc}");
            Some(0x6000_0000 + u64::from(pc) * 8)
        }
    }

    fn tier1_label(low: &Lowered) -> L {
        low.resume
            .iter()
            .find(|(pc, _)| *pc == H)
            .map(|&(_, l)| l)
            .unwrap_or_else(|| low.pc_labels[H as usize].unwrap())
    }

    pub struct Run {
        pub code: u32,
        pub m: Mem,
        pub handler_calls: u32,
        pub exit_pc: Option<u32>,
        pub th_after: Option<(usize, usize)>,
    }

    pub fn run(m: &Mem, kind: Option<Stubs>) -> Run {
        let mut m = m.clone();
        let code_addr = code().as_ptr() as usize;
        let mut low = lowered(code_addr, m.irq.as_ptr());
        let (t, regs) = slice_ir(&low);
        let kinds: Vec<LabelKind> = (0..low.total as u32).map(|pc| low.label_kind(pc)).collect();
        let al = hand_allocation(&t, &regs);
        let res = SliceResolver {
            kinds,
            code: code().to_vec(),
        };
        let (snaps, descs) = build_exits(&t, &al, &res).expect("builds");
        let meta = ExitMeta {
            seg_oracle: 0,
            root_code: code().as_ptr(),
            snaps: snaps.clone(),
            exits: descs,
            vb: Loc::G(VB),
            counters: std::ptr::null(),
            cells: std::ptr::null(),
            peeled: false,
            extent: 0,
            depth: 0,
            frame_cells: false,
            lazy: false,
        };
        let (start, targets, shared) = match kind {
            None => (tier1_label(&low), vec![], false),
            Some(s) => {
                let (entry, targets) =
                    lower_slice(&mut low, &t, &regs, &snaps, s, m.irq.as_ptr() as usize);
                (entry, targets, s == Stubs::Generic)
            }
        };
        let _ = shared;
        let env = m.env();
        let mut emu = Emu::new(&low.prog, start);
        for (a, l) in targets {
            emu.map_target(a, l);
        }
        let base_ptr = env.frame;
        let stack0_of = move |_f: *mut u8| base_ptr.wrapping_sub((5 + 1) * 16);
        let mut h = MetaHelpers {
            meta: &meta,
            stack0_of: &stack0_of,
            stack_size: 5 + 1 + NSLOT,
            calls: 0,
            out: None,
            th_after: None,
        };
        // SAFETY: the buffers in `m` are large enough for every offset the templates, the trace and the helper use; the
        // helper restores into the same buffers (a consistent synthetic thread: a root frame, no inlined frames).
        let u = unsafe { emu.run(&low.prog, &[], &env, 10_000_000, &mut h) };
        let code = match u {
            Until::Stop(Stop::Ret(c)) => c,
            other => panic!("emulator: {other:?}"),
        };
        let calls = h.calls;
        let th_after = h.th_after;
        let exit_pc = h
            .out
            .map(|o| snaps[meta.exits[o.exitno as usize].snap as usize].pc);
        Run {
            code,
            m,
            handler_calls: calls,
            exit_pc,
            th_after,
        }
    }

    pub fn tier1(m: &Mem) -> Run {
        run(m, None)
    }

    pub fn same(a: &Mem, b: &Mem, what: &str) {
        assert_eq!(a.frame, b.frame, "frame, {what}");
        // (the generic path stores `savedpc = &code[pc]`, the inline stub leaves it to tier 1: compared separately)
        let mask = |m: &Mem| {
            let mut c = m.ci.clone();
            c[ci::SAVEDPC / 8] = 0;
            c
        };
        assert_eq!(mask(a), mask(b), "ci, {what}");
        assert_eq!(a.bud.cnt, b.bud.cnt, "budget, {what}");
    }
}

const INTERESTING: [i64; 10] = [
    0,
    1,
    -1,
    2,
    7,
    -100,
    i64::MAX,
    i64::MAX - 1,
    i64::MIN,
    1 << 40,
];

fn random_slice_frame(rng: &mut Rng) -> slice::Mem {
    let mut m = slice::Mem::new();
    let pick = |rng: &mut Rng| -> i64 {
        match rng.below(3) {
            0 => INTERESTING[rng.below(INTERESTING.len() as u64) as usize],
            1 => rng.below(41) as i64 - 20,
            _ => rng.next() as i64,
        }
    };
    let fresh = if rng.next() & 1 == 0 {
        rn_core::jit_layout::callinfo::CIST_FRESH
    } else {
        0
    };
    m.setup_ci(2 | fresh);
    if rng.below(12) == 0 {
        m.set_reg(0, LUA_VNUMFLT, ((rng.next() % 20) as f64).to_bits());
        m.set_reg(1, LUA_VNUMFLT, 1.0f64.to_bits());
        m.set_reg(2, LUA_VNUMFLT, 0.5f64.to_bits());
    } else {
        m.set_reg(0, LUA_VNUMINT, rng.next() % 30);
        m.set_reg(1, LUA_VNUMINT, pick(rng) as u64);
        m.set_reg(2, LUA_VNUMINT, pick(rng) as u64);
    }
    match rng.below(12) {
        0 => m.set_reg(4, LUA_VNUMFLT, 2.5f64.to_bits()),
        1 => m.set_reg(4, LUA_VNIL, 0),
        _ => m.set_reg(4, LUA_VNUMINT, pick(rng) as u64),
    }
    m.bud.cnt = if rng.below(5) < 2 {
        rng.below(16) as i32
    } else {
        i32::MAX
    };
    m
}

/// The slice with generic stubs + shared routine + handler, with inline stubs, and tier 1 alone, on random frames: the
/// same exit code, frame, `CallInfo`, budget word and `savedpc` (landing label and budget exactness against the P0
/// count model); the handler also leaves `thread.ci` / `thread.top` as 5.5 says.
#[test]
fn slice_generic_stubs_equal_inline_stubs_and_tier1() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let n = if cfg!(miri) { 6 } else { 100 };
    let mut via_handler = 0;
    for case in 0..n {
        let m = random_slice_frame(&mut rng);
        let t1 = slice::tier1(&m);
        let inl = slice::run(&m, Some(slice::Stubs::Inline));
        let gen = slice::run(&m, Some(slice::Stubs::Generic));
        assert_eq!(inl.code, t1.code, "exit code inline vs tier 1, case {case}");
        slice::same(&inl.m, &t1.m, &format!("inline vs tier 1, case {case}"));
        assert_eq!(
            gen.code, inl.code,
            "exit code generic vs inline, case {case}"
        );
        slice::same(&gen.m, &inl.m, &format!("generic vs inline, case {case}"));
        assert_eq!(inl.handler_calls, 0);
        if gen.handler_calls > 0 {
            via_handler += 1;
            let at_exit = slice::code().as_ptr() as usize + 4 * gen.exit_pc.unwrap() as usize;
            assert!(
                gen.m.savedpc() == t1.m.savedpc() || gen.m.savedpc() == at_exit,
                "savedpc is the exit pc (or what tier 1 wrote later), case {case}"
            );
            // restore left thread.ci = the root frame and thread.top = its top
            // (tier 1 pops the frame later at RETURN1: look at the words right after the handler)
            assert_eq!(
                gen.th_after,
                Some((gen.m.ci.as_ptr() as usize, 5 + 1 + 40 - 4)),
                "thread.ci = the root frame, thread.top = ci.top, case {case}"
            );
        }
    }
    assert!(via_handler > 30.min(n / 2), "{via_handler}");
}

#[test]
fn slice_count_model_matches_the_snapshots() {
    let m = slice::Mem::new();
    let words: Vec<u32> = slice::bytecode().iter().map(|i| i.0).collect();
    let low = rn_jit::lower::lower(
        &words,
        rn_jit::lower::LowerCfg {
            calls: None,
            code_addr: slice::code().as_ptr() as usize,
            interrupt_addr: m.irq.as_ptr() as usize,
            consts: &[],
            t15_warmup: None,
        },
    )
    .unwrap();
    let (t, _regs) = slice::slice_ir(&low);
    // refund = n_charged - fetches(pc) - land(pc), per snapshot: S0 nothing charged, S0R the pre-roll charge, S1 and S2 the
    // FORLOOP mid-block exits, SH the loop top (uncharged)
    let fl = slice::FORLOOP_PC;
    let mid = 2 - low.fetches(&[1]) as i32 - low.land(fl) as i32;
    assert_eq!(mid, 2 - 1 - 1);
    let refunds: Vec<i32> = t.snaps.iter().map(|s| s.refund).collect();
    assert_eq!(refunds, vec![0, 2, mid, 0, mid]);
    assert_eq!(low.land(1), 0);
}

#[test]
fn slice_exit_through_the_handler_reports_exit_numbers_and_pc() {
    // a frame that makes the loop leave through S1 (count == 0 in the pre-roll): the handler saw exit number 2
    let mut m = slice::Mem::new();
    m.setup_ci(2);
    m.set_reg(0, LUA_VNUMINT, 0);
    m.set_reg(1, LUA_VNUMINT, 1);
    m.set_reg(2, LUA_VNUMINT, 5);
    m.set_reg(4, LUA_VNUMINT, 100);
    let gen = slice::run(&m, Some(slice::Stubs::Generic));
    let t1 = slice::tier1(&m);
    assert_eq!(gen.handler_calls, 1);
    slice::same(&gen.m, &t1.m, "S1 exit");
    // tier 1 finished the loop from the FORLOOP: 100 + 5 (one ADD)
    assert_eq!(
        gen.m.frame[3], 105,
        "RETURN1 moved r4 into the function slot"
    );
}

// ---- 5. the handler with real traces: counting, hot exits, deferred flushes -------------------------------------

mod handler {
    use super::*;
    use rn_core::state::Thread;
    use rn_jit::rt::set_trace;
    use rn_jit::t15::runtime::{T15Config, T15Mode};
    use rn_jit::tr::cache::{self as c, ExitSite, ProtoRef, TrConfig, TrId, TrMode, TraceInstall};
    use rn_jit::tr::exit::{ExitDesc, ExitMeta};
    use std::sync::atomic::{AtomicU64, Ordering};

    thread_local! {
        static GRABBED: RefCell<Option<ProtoRef>> = const { RefCell::new(None) };
    }

    fn n_grab(st: &mut State) -> LuaResult<usize> {
        if let Some(p) = c::caller_proto(st) {
            GRABBED.with(|g| *g.borrow_mut() = Some(p));
        }
        Ok(0)
    }

    pub fn state(cfg: TrConfig) -> LuaState {
        init();
        let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
        rn_jit::set_mode(&mut st, Mode::Always);
        rn_jit::set_t15(&mut st, T15Config::for_mode(T15Mode::Off));
        set_trace(&mut st, cfg);
        let g = st.globals();
        st.raw_set_str(g, b"grab", Value::native(n_grab)).unwrap();
        st
    }

    fn with_f(cfg: TrConfig) -> (LuaState, ProtoRef, u32) {
        let mut st = state(cfg);
        let src = "function f(n) local s = 0 for i = 1, n do s = s + i end grab() return s end return f(10)";
        assert_eq!(exec(&mut st, src), "55");
        let f = GRABBED.with(|g| g.borrow().expect("grab was reached"));
        let infos = c::head_infos(&mut st, f);
        assert_eq!(infos.len(), 1);
        let pc = infos[0].pc;
        (st, f, pc)
    }

    static CODE: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    fn code() -> &'static [u32] {
        CODE.get_or_init(|| vec![0u32; 64])
    }

    /// The trace calls this as its exit handler (`a` = trace slot): `exit_entry` on a private budget word + area.
    unsafe extern "C" fn shim(th: *mut Thread, ci: *mut CallInfo, slot: u32, exitno: u32) -> u32 {
        let mut bud = Bud::new();
        bud.cnt = 1000;
        bud.area.trace_id = slot;
        bud.area.exitno = exitno;
        // SAFETY: called from the fake trace with the running thread and its frame; the area follows its budget word.
        let h = unsafe { exit_entry(th, ci, (&raw mut *bud).cast::<i32>()) };
        HANDLED.with(|x| x.borrow_mut().push((h.out.exitno, h.out.count, h.flushed)));
        0
    }

    thread_local! {
        static HANDLED: RefCell<Vec<(u32, u32, bool)>> = const { RefCell::new(Vec::new()) };
    }

    static PROBE_GRAVE: AtomicU64 = AtomicU64::new(0);
    static PROBE_STALE: AtomicU64 = AtomicU64::new(0);
    static PROBE_MAPPED: AtomicU64 = AtomicU64::new(0);

    fn probe(st: &mut State, id: TrId) {
        let g = c::grave_blocks(st);
        PROBE_GRAVE.store(g.len() as u64, Ordering::Relaxed);
        PROBE_STALE.store(u64::from(!c::trace_live(st, id)), Ordering::Relaxed);
        // the parked block is still mapped and readable (the handler was called from it)
        #[cfg(not(miri))]
        if let Some(&(addr, size)) = g.first() {
            let maps = std::fs::read_to_string("/proc/self/maps").unwrap_or_default();
            let ok = maps.lines().any(|l| {
                let mut it = l.split_whitespace();
                let range = it.next().unwrap_or("");
                let perms = it.next().unwrap_or("");
                let (a, b) = range.split_once('-').unwrap_or(("0", "0"));
                let (a, b) = (
                    usize::from_str_radix(a, 16).unwrap_or(0),
                    usize::from_str_radix(b, 16).unwrap_or(0),
                );
                a <= addr && addr + size <= b && perms.starts_with('r')
            });
            PROBE_MAPPED.store(u64::from(ok), Ordering::Relaxed);
        }
    }

    fn install(
        st: &mut LuaState,
        f: ProtoRef,
        pc: u32,
        site: ExitSite,
        counter: &AtomicU64,
    ) -> TrId {
        install_h(st, f, pc, site, counter, shim as *const () as usize as u64).0
    }

    /// Installs a fake trace whose exit helper is `helper`; returns the id and a weak handle on its exit metadata.
    fn install_h(
        st: &mut LuaState,
        f: ProtoRef,
        pc: u32,
        site: ExitSite,
        counter: &AtomicU64,
        helper: u64,
    ) -> (TrId, std::sync::Weak<ExitMeta>) {
        let id = c::begin_recording(st, f, pc).expect("begin");
        let ctrs = c::alloc_exit_counters(st, id, 1).expect("counters");
        let land = c::head_infos(st, f)[0].pcl_addr;
        let h = Some((helper, id.slot, 0));
        let ti = TraceInstall {
            code: c::fake_trace_code(std::ptr::from_ref(counter) as usize, land, h),
            entry_off: 0,
            inner_off: 0,
            rec_ret: false,
            text: Some("fake trace".into()),
            hash: 0,
            weight: 0,
        };
        let tid = c::install(st, id, ti).expect("install");
        let meta = ExitMeta {
            seg_oracle: 0,
            root_code: code().as_ptr(),
            snaps: vec![ExitSnap {
                pc: 0,
                slots: vec![],
                frames: vec![],
                refund: 0,
                land: Land::Pre,
                land_addr: land as u64,
                usable: true,
                link_cell: 0,
                root_code: std::ptr::null(),
                seg_shift: 0,
                seg_pops: 0,
        sunk: Vec::new(),
        top: None,
            }],
            exits: vec![ExitDesc {
                snap: 0,
                site,
                pre: false,
            }],
            vb: Loc::None,
            counters: ctrs,
            cells: std::ptr::null(),
            peeled: false,
            extent: 0,
            depth: 0,
            frame_cells: false,
            lazy: false,
        };
        let m = c::attach_exits(st, tid, meta).expect("live");
        (tid, std::sync::Arc::downgrade(&m))
    }

    fn cfg(hot_exit: u32) -> TrConfig {
        let mut c = TrConfig::for_mode(TrMode::On);
        c.hot_loop = 1000;
        c.hot_exit = hot_exit;
        // K3a: these tests drive and count the loop heads themselves; the function-entry heads that native recursion adds (the shipped `recnat` /
        // `recany`) are covered by tr_core and tr_asm, so they are switched off here explicitly
        c.limits.recnat = 0;
        c.limits.recany = 0;
        c
    }

    /// TR18: a hot type-guard exit flushes the trace from inside the handler. The head transition is immediate, the
    /// block stays mapped (parked) while the handler runs and until the next safe point.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "needs rn-std (extern static stdin) and executable memory"
    )]
    fn flush_requested_inside_the_handler_is_deferred_and_the_trace_memory_stays_valid() {
        let (mut st, f, pc) = with_f(cfg(1));
        PROBE_GRAVE.store(99, Ordering::Relaxed);
        HANDLED.with(|h| h.borrow_mut().clear());
        set_probe(Some(probe));
        let n = AtomicU64::new(0);
        let id = install(&mut st, f, pc, ExitSite::TypeGuard, &n);
        let (blk, _) = c::trace_block(&mut st, id).unwrap();
        assert_eq!(exec(&mut st, "return f(10)"), "55");
        set_probe(None);
        // inside the handler: the id was already stale, the block was parked in the graveyard and mapped
        assert_eq!(
            PROBE_STALE.load(Ordering::Relaxed),
            1,
            "head detached at once"
        );
        assert_eq!(
            PROBE_GRAVE.load(Ordering::Relaxed),
            1,
            "the running block was parked, not freed"
        );
        #[cfg(not(miri))]
        assert_eq!(
            PROBE_MAPPED.load(Ordering::Relaxed),
            1,
            "the parked block is still mapped"
        );
        let _ = blk;
        // one entry, one report; afterwards (a safe point ran) nothing is parked
        assert_eq!(n.load(Ordering::Relaxed), 1);
        assert_eq!(HANDLED.with(|h| h.borrow().clone()), vec![(0, 1, true)]);
        assert!(!c::trace_live(&mut st, id));
        assert_eq!(c::counts(&mut st), (0, 0));
        assert_eq!(c::tr_stats(&mut st).deferred, 1);
        assert_eq!(c::tr_stats(&mut st).exit_type_hot, 1);
    }

    /// Exits of an `Other` site are counted; the threshold is reported once and the trace stays.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "needs rn-std (extern static stdin) and executable memory"
    )]
    fn exits_are_counted_and_other_sites_report_once_without_flushing() {
        let (mut st, f, pc) = with_f(cfg(2));
        HANDLED.with(|h| h.borrow_mut().clear());
        let n = AtomicU64::new(0);
        let id = install(&mut st, f, pc, ExitSite::Other, &n);
        assert_eq!(exec(&mut st, "return f(10) + f(10) + f(10)"), "165");
        // (the fake trace is entered at every back-edge: 3 calls x 10 iterations)
        assert_eq!(n.load(Ordering::Relaxed), 30);
        let want: Vec<(u32, u32, bool)> = (1..=30).map(|k| (0, k, false)).collect();
        assert_eq!(
            HANDLED.with(|h| h.borrow().clone()),
            want,
            "(flushed is false: an Other site only counts)"
        );
        assert_eq!(c::trace_exit_counters(&mut st, id), vec![30]);
        assert!(c::trace_live(&mut st, id));
        let s = c::tr_stats(&mut st);
        assert_eq!((s.exit_other_hot, s.exit_type_hot, s.deferred), (1, 0, 0));
    }

    /// Installs a fake root trace with `n` exits and exit cells (initially the tier-1 landing address); returns its id,
    /// the cells and the landing address.
    fn install_with_cells(
        st: &mut LuaState,
        f: ProtoRef,
        pc: u32,
        n: usize,
        counter: &AtomicU64,
    ) -> (TrId, *const std::cell::Cell<usize>, usize) {
        install_with_cells_h(st, f, pc, n, counter, None)
    }

    /// [`install_with_cells`] whose fake trace calls `helper` (the exit handler shim) with exit number 0.
    fn install_with_cells_h(
        st: &mut LuaState,
        f: ProtoRef,
        pc: u32,
        n: usize,
        counter: &AtomicU64,
        helper: Option<u64>,
    ) -> (TrId, *const std::cell::Cell<usize>, usize) {
        install_with_cells_hl(st, f, pc, n, counter, helper, 0)
    }

    /// [`install_with_cells_h`] whose exit snapshots are linking exits to the dispatch cell at `link_cell` (F2b; 0: none).
    fn install_with_cells_hl(
        st: &mut LuaState,
        f: ProtoRef,
        pc: u32,
        n: usize,
        counter: &AtomicU64,
        helper: Option<u64>,
        link_cell: u64,
    ) -> (TrId, *const std::cell::Cell<usize>, usize) {
        let id = c::begin_recording(st, f, pc).expect("begin");
        let ctrs = c::alloc_exit_counters(st, id, n).expect("counters");
        let land = c::head_infos(st, f)[0].pcl_addr;
        let cells = c::alloc_exit_cells(st, id, &vec![land; n]).expect("cells");
        let ti = TraceInstall {
            code: c::fake_trace_code(
                std::ptr::from_ref(counter) as usize,
                land,
                helper.map(|h| (h, id.slot, 0)),
            ),
            entry_off: 0,
            inner_off: 0,
            rec_ret: false,
            text: None,
            hash: 0,
            weight: 0,
        };
        let tid = c::install(st, id, ti).expect("install");
        let snaps = (0..n)
            .map(|_| ExitSnap {
                pc: 0,
                slots: vec![],
                frames: vec![],
                refund: 0,
                land: Land::Pre,
                land_addr: land as u64,
                usable: true,
                link_cell,
                root_code: std::ptr::null(),
                seg_shift: 0,
                seg_pops: 0,
                sunk: Vec::new(),
                top: None,
            })
            .collect();
        let exits = (0..n as u32)
            .map(|snap| ExitDesc {
                snap,
                site: ExitSite::Other,
                pre: false,
            })
            .collect();
        let meta = ExitMeta {
            seg_oracle: 0,
            root_code: code().as_ptr(),
            snaps,
            exits,
            vb: Loc::None,
            counters: ctrs,
            cells,
            peeled: true,
            extent: 0,
            depth: 0,
            frame_cells: false,
            lazy: false,
        };
        c::attach_exits(st, tid, meta).expect("live");
        (tid, cells, land)
    }

    /// Installs a fake side trace on exit `k` of `parent` (which has `n` exits).
    fn install_side(
        st: &mut LuaState,
        parent: TrId,
        k: u32,
        n: usize,
        land: usize,
        counter: &AtomicU64,
    ) -> TrId {
        let id = c::begin_side_recording(st, parent, k).expect("begin side");
        let ctrs = c::alloc_exit_counters(st, id, n).expect("counters");
        let cells = c::alloc_exit_cells(st, id, &vec![land; n]).expect("cells");
        let ti = TraceInstall {
            code: c::fake_trace_code(std::ptr::from_ref(counter) as usize, land, None),
            entry_off: 0,
            inner_off: 0,
            rec_ret: false,
            text: None,
            hash: 0,
            weight: 0,
        };
        let tid = c::install(st, id, ti).expect("install side");
        let meta = ExitMeta {
            seg_oracle: 0,
            root_code: code().as_ptr(),
            snaps: (0..n)
                .map(|_| ExitSnap {
                    pc: 0,
                    slots: vec![],
                    frames: vec![],
                    refund: 0,
                    land: Land::Pre,
                    land_addr: land as u64,
                    usable: true,
                    link_cell: 0,
                    root_code: std::ptr::null(),
                    seg_shift: 0,
                    seg_pops: 0,
        sunk: Vec::new(),
        top: None,
                })
                .collect(),
            exits: (0..n as u32)
                .map(|snap| ExitDesc {
                    snap,
                    site: ExitSite::Other,
                    pre: false,
                })
                .collect(),
            vb: Loc::None,
            counters: ctrs,
            cells,
            peeled: false,
            extent: 0,
            depth: 0,
            frame_cells: false,
            lazy: false,
        };
        c::attach_exits(st, tid, meta).expect("live");
        tid
    }

    thread_local! {
        static CELLS_SEEN: RefCell<Option<(usize, TrId, Vec<usize>)>> = const { RefCell::new(None) };
    }

    /// Inside the handler: flush the family of the exiting trace (deferred, TR18) and look at its exit cells, which a
    /// stub still running in the trace would read.
    fn probe_flush_and_look(st: &mut State, id: TrId) {
        let Some((cells, _, _)) = CELLS_SEEN.with(|c| c.borrow().clone()) else {
            return;
        };
        if !c::trace_live(st, id) {
            return;
        }
        c::request_flush(st, id, c::FlushWhy::Dep);
        // SAFETY: the trace is parked in the graveyard (the handler is active), so its cells are still allocated.
        let seen: Vec<usize> = (0..2)
            .map(|k| unsafe { &*(cells as *const std::cell::Cell<usize>).add(k) }.get())
            .collect();
        CELLS_SEEN.with(|c| *c.borrow_mut() = Some((cells, id, seen)));
    }

    /// F2a (TR18): a trace flushed inside its own exit handler (its stubs may still run) has all its exit cells put back
    /// to the tier-1 addresses at once, so that no stub of it can continue in another trace of the family, whose block
    /// is about to be freed. Mutant `flush_keeps_cells`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "needs rn-std (extern static stdin) and executable memory"
    )]
    fn a_trace_flushed_inside_its_handler_leaves_its_cells_at_the_tier1_addresses() {
        let (mut st, f, pc) = with_f(cfg(1000));
        let n = AtomicU64::new(0);
        let (root, cells, land) = install_with_cells_h(
            &mut st,
            f,
            pc,
            2,
            &n,
            Some(shim as *const () as usize as u64),
        );
        let _side = install_side(&mut st, root, 0, 2, land, &n);
        let (entry, _) = c::trace_block(&mut st, _side).unwrap();
        // exit 0 now continues in the side trace
        // SAFETY: the cells belong to the live root trace.
        assert_eq!(unsafe { &*cells }.get(), entry);
        CELLS_SEEN.with(|c| *c.borrow_mut() = Some((cells as usize, root, Vec::new())));
        set_probe(Some(probe_flush_and_look));
        assert_eq!(exec(&mut st, "return f(10)"), "55");
        set_probe(None);
        let seen = CELLS_SEEN.with(|c| c.borrow_mut().take()).unwrap().2;
        assert_eq!(seen, vec![land, land], "the cells were reset when the family was flushed");
        assert_eq!(c::counts(&mut st), (0, 2), "parked until the next safe point");
        assert_eq!(exec(&mut st, "return f(3)"), "6");
        assert_eq!(c::counts(&mut st), (0, 0));
    }

    /// F2a: the ineffective-trace rule needs 90% of the entries to die in the pre-roll, 50% when a side trace hangs off a
    /// type-guard exit of the root (the two ping-pong, which the prototype's re-record on a hot type-guard exit cured at
    /// once). Mutant `ineffective_pct_ignored`.
    #[test]
    fn ineffective_ratio_is_lower_with_a_type_guard_side_trace() {
        assert!(!c::pre_exit_flush_due(600, 1000, false));
        assert!(c::pre_exit_flush_due(600, 1000, true));
        assert!(c::pre_exit_flush_due(900, 1000, false));
        assert!(!c::pre_exit_flush_due(400, 1000, true));
        // N5: with body exits (iterations completed between the hops) the lowered ratio does not apply
        assert!(c::pre_exit_flush_due_n(600, 1000, true, false, 0));
        assert!(c::pre_exit_flush_due_n(600, 1000, true, false, 100));
        assert!(!c::pre_exit_flush_due_n(600, 1000, true, false, 600));
        assert!(c::pre_exit_flush_due_n(900, 1000, true, false, 900), "90% always flushes");
    }

    /// F2a (TR8, TR18): a side trace is reached through its parent's exit cell only; flushing the side trace alone puts
    /// the tier-1 landing address back at once, a recording cannot start on an exit that has a side trace, and a flush of
    /// the root takes its whole family (children and grandchildren) with it. Mutants `side_cell_stale`,
    /// `side_family_flush_skip`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "needs rn-std (extern static stdin) and executable memory"
    )]
    fn side_traces_attach_to_exit_cells_detach_alone_and_flush_with_their_family() {
        let (mut st, f, pc) = with_f(cfg(1000));
        let n = AtomicU64::new(0);
        let (root, cells, land) = install_with_cells(&mut st, f, pc, 2, &n);
        let cell = |k: usize| {
            // SAFETY: the cells belong to the live root trace (checked by the callers).
            unsafe { &*cells.add(k) }.get()
        };
        assert_eq!((cell(0), cell(1)), (land, land));
        // a side trace on exit 1
        let s1 = install_side(&mut st, root, 1, 2, land, &n);
        let (entry1, _) = c::trace_block(&mut st, s1).unwrap();
        assert_eq!((cell(0), cell(1)), (land, entry1), "exit 1 continues in the side trace");
        assert_eq!(
            c::begin_side_recording(&mut st, root, 1).err(),
            Some(c::Begin::NotNow),
            "the exit has its side trace"
        );
        assert_eq!(c::counts(&mut st), (2, 0));
        // the side trace is flushed alone (a dependency freed, say): the exit lands in tier 1 again
        c::request_flush(&mut st, s1, c::FlushWhy::Dep);
        assert!(c::trace_live(&mut st, root));
        assert!(!c::trace_live(&mut st, s1));
        assert_eq!((cell(0), cell(1)), (land, land), "the cell is back at the tier-1 address");
        assert_eq!(c::counts(&mut st), (1, 0));
        // a new one, and a side trace of the side trace
        let s2 = install_side(&mut st, root, 1, 2, land, &n);
        let s3 = install_side(&mut st, s2, 0, 2, land, &n);
        assert_eq!(c::counts(&mut st), (3, 0));
        let st_ = c::tr_stats(&mut st);
        assert_eq!((st_.side_started, st_.side_installed), (3, 3));
        // the root goes: so does the family
        c::request_flush(&mut st, root, c::FlushWhy::Dep);
        assert!(!c::trace_live(&mut st, s2) && !c::trace_live(&mut st, s3));
        assert_eq!(c::counts(&mut st), (0, 0), "no side trace outlives its root");
    }

    /// F2b: a linking exit continues at the value of the dispatch cell its snapshot names, not at the tier-1 landing
    /// address: with the cell pointing at the head's counting stub the stub runs (its counter moves), with the cell at the
    /// landing address it does not; a cell changed afterwards (a flush of the target) is followed. Mutant
    /// `frame_link_cell_ignored`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "needs rn-std (extern static stdin) and executable memory"
    )]
    fn a_linking_exit_continues_at_the_dispatch_cell_of_its_target() {
        let run = |to_stub: bool| {
            let (mut st, f, pc) = with_f(cfg(1000));
            let n = AtomicU64::new(0);
            let hi = c::head_infos(&mut st, f)[0].clone();
            let want = if to_stub { hi.c_addr } else { hi.pcl_addr };
            let target: &'static std::cell::Cell<usize> =
                super::keep(Box::leak(Box::new(std::cell::Cell::new(want))));
            let (id, _cells, land) = install_with_cells_hl(
                &mut st,
                f,
                pc,
                1,
                &n,
                Some(jit_tr_exit as *const () as usize as u64),
                std::ptr::from_ref(target) as u64,
            );
            assert_ne!(land, hi.c_addr);
            let cnt = rn_jit::rt::trace_budget_ptr(&mut st);
            // SAFETY: as in `jit_tr_exit_runs_natively_on_the_real_jitdata_area`.
            let area = unsafe {
                &mut *cnt
                    .cast::<u8>()
                    .add(exit_area::FROM_R15 as usize)
                    .cast::<ExitArea>()
            };
            area.trace_id = id.slot;
            area.exitno = 0;
            area.args[0] = 0;
            assert_eq!(exec(&mut st, "return f(10)"), "55");
            (area.args[0] as usize, want)
        };
        let (got, want) = run(false);
        assert_eq!(got, want, "the cell holds the landing address");
        let (got, want) = run(true);
        assert_eq!(got, want, "the exit continues where the cell says, not at the tier-1 label");
    }

    /// `jit_tr_exit` runs natively from a fake trace on the real `JitData` area (no test-owned `Bud`): the stub's
    /// stores (trace id, exit number) are made into the real area beforehand; the landing address comes back in
    /// `args[0]`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "needs rn-std (extern static stdin) and executable memory"
    )]
    fn jit_tr_exit_runs_natively_on_the_real_jitdata_area() {
        let (mut st, f, pc) = with_f(cfg(1000));
        let n = AtomicU64::new(0);
        let (id, _w) = install_h(
            &mut st,
            f,
            pc,
            ExitSite::Other,
            &n,
            jit_tr_exit as *const () as usize as u64,
        );
        let land = c::head_infos(&mut st, f)[0].pcl_addr;
        let cnt = rn_jit::rt::trace_budget_ptr(&mut st);
        // SAFETY: `cnt` is the budget word of the live JitData; the area follows it (frozen offset); no activation
        // is running.
        let area = unsafe {
            &mut *cnt
                .cast::<u8>()
                .add(exit_area::FROM_R15 as usize)
                .cast::<ExitArea>()
        };
        area.trace_id = id.slot;
        area.exitno = 0;
        area.args[0] = 0;
        assert_eq!(exec(&mut st, "return f(10)"), "55");
        assert_eq!(n.load(Ordering::Relaxed), 10);
        assert_eq!(c::trace_exit_counters(&mut st, id), vec![10]);
        // SAFETY: as above.
        let area = unsafe {
            &*cnt
                .cast::<u8>()
                .add(exit_area::FROM_R15 as usize)
                .cast::<ExitArea>()
        };
        assert_eq!(area.args[0], land as u64, "landing address left in args[0]");
    }

    /// The exit metadata lives in the trace: dropped exactly once with it, and it survives a flush requested inside
    /// the handler (the handler's clone; the trace itself is parked in the graveyard).
    #[test]
    #[cfg_attr(
        miri,
        ignore = "needs rn-std (extern static stdin) and executable memory"
    )]
    fn exit_metadata_is_dropped_once_with_the_trace_and_survives_an_inner_flush() {
        let (mut st, f, pc) = with_f(cfg(1));
        set_probe(Some(meta_probe));
        META_ALIVE.store(9, Ordering::Relaxed);
        let n = AtomicU64::new(0);
        let (id, w) = install_h(
            &mut st,
            f,
            pc,
            ExitSite::TypeGuard,
            &n,
            shim as *const () as usize as u64,
        );
        assert_eq!(w.strong_count(), 1, "owned by the trace only");
        META_WEAK.with(|m| *m.borrow_mut() = Some(w.clone()));
        assert!(c::exit_meta(&mut st, id.slot).is_some());
        assert_eq!(exec(&mut st, "return f(10)"), "55");
        set_probe(None);
        // inside the handler the trace was stale and parked, its metadata still readable (P-L: the handler borrows it
        // from the parked trace, no reference count of its own)
        assert_eq!(
            META_ALIVE.load(Ordering::Relaxed),
            1,
            "the grave (trace) owns it; the handler holds a borrow"
        );
        assert_eq!(c::counts(&mut st), (0, 0));
        assert_eq!(w.strong_count(), 0, "dropped with the retired trace");
        assert!(w.upgrade().is_none());
        assert!(c::exit_meta(&mut st, id.slot).is_none());

        // plain flush: dropped exactly once with the trace
        let n2 = AtomicU64::new(0);
        let (id2, w2) = install_h(
            &mut st,
            f,
            pc,
            ExitSite::Other,
            &n2,
            shim as *const () as usize as u64,
        );
        assert_eq!(w2.strong_count(), 1);
        c::request_flush(&mut st, id2, c::FlushWhy::Dep);
        assert_eq!(w2.strong_count(), 0);
    }

    static META_ALIVE: AtomicU64 = AtomicU64::new(0);
    thread_local! {
        static META_WEAK: RefCell<Option<std::sync::Weak<ExitMeta>>> = const { RefCell::new(None) };
    }
    /// Inside the handler: the trace is stale and parked (graveyard owner) and the handler holds a clone.
    fn meta_probe(_st: &mut State, _id: TrId) {
        let n = META_WEAK.with(|w| w.borrow().as_ref().map_or(0, |w| w.strong_count()));
        META_ALIVE.store(n as u64, Ordering::Relaxed);
    }

    #[test]
    fn is_hot_policy() {
        assert!(
            !is_hot(ExitSite::TypeGuard, 1, 2)
                && is_hot(ExitSite::TypeGuard, 2, 2)
                && is_hot(ExitSite::TypeGuard, 9, 2)
        );
        assert!(
            !is_hot(ExitSite::Other, 1, 2)
                && is_hot(ExitSite::Other, 2, 2)
                && !is_hot(ExitSite::Other, 3, 2)
        );
    }
}

// ---- mutants ----------------------------------------------------------------------------------------------------

/// `frame_savedpc_caller_off` and `frame_nresults_off`: each is caught by the model test (frames 1..4) and the twin
/// test; a child process selects the mutant and the test must FAIL there while passing in a baseline child.
#[cfg(feature = "tr_mutants")]
#[test]
fn mutants_are_caught() {
    if std::env::var("TR_EXIT_CHILD").is_ok() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let cases = [
        ("frame_savedpc_caller_off", "restore_matches_the_model_for_0_to_4_frames"),
        ("frame_savedpc_caller_off", "twin_restore_equals_the_interpreter_for_0_to_4_frames"),
        ("frame_nresults_off", "restore_matches_the_model_for_0_to_4_frames"),
        ("frame_nresults_off", "twin_restore_equals_the_interpreter_for_0_to_4_frames"),
        ("exit_kcache_wrong", "restore_matches_the_model_for_0_to_4_frames"),
        ("exit_kcache_wrong", "twin_restore_equals_the_interpreter_for_0_to_4_frames"),
        ("exit_top_off_by_one", "restore_matches_the_model_for_0_to_4_frames"),
        ("exit_top_off_by_one", "twin_resume_from_the_restored_frames_equals_the_interpreter_alone"),
        ("exit_th_top_not_written", "restore_matches_the_model_for_0_to_4_frames"),
        ("exit_th_top_not_written", "twin_restore_equals_the_interpreter_for_0_to_4_frames"),
        ("exit_refund_sign_flipped", "restore_matches_the_model_for_0_to_4_frames"),
        ("exit_refund_sign_flipped", "inline_stub_equals_restore_for_every_budget_kind"),
        ("flush_in_handler", "handler::flush_requested_inside_the_handler_is_deferred_and_the_trace_memory_stays_valid"),
    ];
    for (mutant, test) in cases {
        for m in ["", mutant] {
            let out = std::process::Command::new(&exe)
                .args(["--exact", test, "--test-threads=1"])
                .env("TR_EXIT_CHILD", "1")
                .env("RUSTYNAIL_JIT_MUT", m)
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            if m.is_empty() {
                assert!(
                    out.status.success(),
                    "baseline child {test} failed:\n{}",
                    String::from_utf8_lossy(&out.stdout)
                );
            } else {
                assert!(
                    !out.status.success(),
                    "mutant {mutant} was NOT caught by {test}"
                );
            }
        }
    }
}

/// P-B: the guard `NE %alloc, 0` behind a class-A allocation makes its snapshot an allocation site (no side trace, the exit-storm rule
/// watches it); a snapshot shared with another guard stays `Other`.
#[test]
fn pb_allocation_guards_are_alloc_sites() {
    use rn_jit::tr::cache::ExitSite;
    use rn_jit::tr::ir::*;
    let mut ir = TraceIr::default();
    let s0 = ir.snapshot(Default::default(), &[]);
    let s1 = ir.snapshot(Default::default(), &[]);
    let k = ir.kint(5);
    let z = ir.kint(0);
    let mk = |op, t, flags, a, b, aux| Ins { op, t, flags, a, b, prev: NOREF, aux };
    let c = ir.emit(mk(TOp::Snew, Ty::ShrStr, 0, k, NOREF, 0));
    ir.emit(mk(TOp::Ne, Ty::I64, F_GUARD, c, z, guard_aux(s0, 0)));
    let d = ir.emit(mk(TOp::Snew, Ty::ShrStr, 0, k, NOREF, 0));
    ir.emit(mk(TOp::Ne, Ty::I64, F_GUARD, d, z, guard_aux(s1, 0)));
    ir.emit(mk(TOp::Lt, Ty::I64, F_GUARD, k, z, guard_aux(s1, 0)));
    let sites = rn_jit::tr::exit::site_classes(&ir);
    assert_eq!(sites[0], ExitSite::Alloc);
    assert_eq!(sites[1], ExitSite::Other);
}

// ---- BUNDLE-K L1 (`lazyci`): `lazy_leave` completes a chain of lazily pushed frames exactly as the interpreter built it ----------------

/// `lzleave(depth, new)`: called from the leaf of a recursion run by the interpreter. The frames from the leaf down to (excluding) the frame
/// `depth` levels below it get every field a lazy push does not store poisoned (`top`, `kcache`, `trap`, `callstatus`; with `new` also the
/// leaf's `savedpc`), `lazy_leave` runs, and the chain must be bit-for-bit what the interpreter had built, `thread.ci` the leaf, `thread.top` its
/// `top` (for `new`: the end of its arguments) and the root word the leaf.
fn n_lzleave(st: &mut State) -> LuaResult<usize> {
    let depth = st.arg(0).as_int().unwrap_or(1) as usize;
    let new = st.arg(1).as_int().unwrap_or(0) != 0;
    let nat: *mut CallInfo = st.ci().as_ptr();
    // SAFETY: live nodes of the running thread; the chain below the C frame is the recursion.
    unsafe {
        let inner = (*nat).previous;
        let mut root = inner;
        for _ in 0..depth {
            root = (*root).previous;
        }
        let mut saved = Vec::new();
        let mut c = inner;
        while c != root {
            saved.push((c, (*c).top.0, (*c).kcache, (*c).trap, (*c).callstatus, (*c).savedpc));
            c = (*c).previous;
        }
        let (th_ci, th_top) = (st.ci(), st.top());
        for &(c, ..) in &saved {
            (*c).top = StkId(0x7777);
            (*c).kcache = 0xBAD8 as *const Value;
            (*c).trap = 55;
            (*c).callstatus = 0xDEAD;
        }
        if new {
            (*inner).savedpc = 0xBAD0 as *const u32;
        }
        let th = rn_core::jit_hooks::thread_ptr(st).cast::<rn_core::state::Thread>();
        let mut area = ExitArea::default();
        area.lazy_root = root as u64;
        lazy_leave(th, inner, &mut area, u32::from(new));
        let mut bad = Vec::new();
        for &(c, top, k, trap, cs, pc) in &saved {
            if (*c).top.0 != top {
                bad.push(format!("top {} want {}", (*c).top.0, top));
            }
            if (*c).kcache != k {
                bad.push(format!("kcache {:p} want {:p}", (*c).kcache, k));
            }
            // (the interpreter's `trap` of a frame that called a C function is 1; a native push stores 0 and so does the completion)
            let _ = trap;
            if (*c).trap != 0 {
                bad.push(format!("trap {} want 0", (*c).trap));
            }
            if (*c).callstatus != cs {
                bad.push(format!("callstatus {:#x} want {:#x}", (*c).callstatus, cs));
            }
            if !(new && c == inner) && (*c).savedpc != pc {
                bad.push("savedpc changed".into());
            }
        }
        if new {
            let f = (*inner).func;
            let p = st.get(f).as_lclosure().expect("closure").proto();
            if (*inner).savedpc != p.code_ptr() {
                bad.push("a new frame's savedpc is not the start of its code".into());
            }
            if st.top().0 != f.0 + 1 + usize::from(p.numparams) {
                bad.push(format!("thread.top {} want end of the arguments {}", st.top().0, f.0 + 1 + usize::from(p.numparams)));
            }
        } else if st.top().0 != (*inner).top.0 {
            bad.push(format!("thread.top {} want {}", st.top().0, (*inner).top.0));
        }
        if st.ci().as_ptr() != inner {
            bad.push("thread.ci is not the innermost frame".into());
        }
        if area.lazy_root != inner as u64 {
            bad.push(format!("the root word is {:#x}, want the running frame", area.lazy_root));
        }
        // put the thread back for the C frame to return
        st.set_ci(th_ci);
        st.set_top(th_top);
        NOTES.with(|n| n.borrow_mut().extend(bad));
    }
    Ok(0)
}

#[test]
#[cfg_attr(miri, ignore = "opens the io library (extern static `stdin`), which Miri does not support")]
fn lazy_leave_completes_the_chain_like_the_interpreter() {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Off);
    let g = st.globals();
    st.raw_set_str(g, b"lzleave", Value::native(n_lzleave)).unwrap();
    for (depth, new) in [(1, 0), (3, 0), (5, 0), (2, 1), (4, 1)] {
        NOTES.with(|n| n.borrow_mut().clear());
        let src = format!("local function f(n) if n == 0 then lzleave({depth}, {new}) return 0 end return 1 + f(n - 1) end return tostring(f(8))");
        assert_eq!(exec(&mut st, &src), "8", "depth {depth} new {new}");
        let notes = NOTES.with(|n| n.borrow().clone());
        assert!(notes.is_empty(), "depth {depth} new {new}: {notes:?}");
    }
}
