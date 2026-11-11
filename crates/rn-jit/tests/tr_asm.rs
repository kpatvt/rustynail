//! J5-P5b: the trace assembler (`tr::asm`, docs/JIT_TRACE.md 5.3, 7.5-7.8; task P5b, decisions in
//! docs/JIT_TRACE_REF.md "P5b decisions").
//!
//! 1. `emu_*`: assembled micro-ops in the emulator on synthetic memory against `tr::interp` (the reference semantics of
//!    the IR) on random IR from the P2 generator, in both layouts (peeled with PHIs, rolled with a writeback list), with
//!    random allocations (`+randra`), inline and generic exit stubs: exit snapshot, restored frame, budget word, table
//!    and upvalue memory.
//! 2. `native_*`: the same programs natively (`x86::emit` + the thunk): equal to the same reference.
//! 3. `hand_*`: hand-built IR for entry checks (TR6), the budget, polls, barriers, range guards, hot-exit diverts,
//!    `+exitstorm`, entry-failure bookkeeping.
//! 4. `e2e_*` (below): recorded traces of real Lua loops installed and entered through the dispatch cells of a real
//!    state, with forced guard failures, against the interpreter run without traces.
//!
//! Mutants (feature `tr_mutants`): `mutants_are_caught` runs the named tests in a child process with the mutant selected
//! and requires them to fail there and pass in the baseline child.
#![cfg(feature = "trace")]

use rn_core::jit_hooks::thread as thr;
use rn_core::jit_layout::{callinfo as ci, lclosure, node, table, upval};
use rn_core::state::CallInfo;
use rn_core::value::*;
use rn_jit::emu::{Emu, Env, HelperOut, Helpers, Stop, Until};
use rn_jit::tr::asm::{self, Asm, AsmEnv};
use rn_jit::tr::exit::{self, ExitSnap, ProtoInfoR, Resolver};
use rn_jit::tr::interp::gen::{random_case, GenCfg, Rng};
use rn_jit::tr::interp::{self, MTable, Model, Outcome, Val};
use rn_jit::tr::ir::*;
use rn_jit::tr::ra::{self, Allocation, RaOpts};
use rn_jit::tr::{exit_area, ExitArea, FPool};
use std::cell::{Cell, RefCell};

// ---- the budget mode (PIa) ---------------------------------------------------------------------------------------

thread_local! {
    /// `Some(r)`: the budget lives in register `V<r>` (`RaOpts::reserve_gpr` + `AsmEnv::budget_reg`); `None`: in the
    /// budget word.
    static VB_REG: Cell<Option<u8>> = const { Cell::new(None) };
}

/// F6: the tests leak programs / strings on purpose (`&'static`); recording the address in a static keeps the memory *reachable*, so
/// LeakSanitizer (scripts/asan-jit.sh) does not report it.
static KEPT: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
fn keep<T: ?Sized>(r: &'static T) -> &'static T {
    KEPT.lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((r as *const T).cast::<u8>() as usize);
    r
}

fn vb_reg() -> Option<u8> {
    VB_REG.with(|v| v.get())
}

/// The location of the budget at exits in the current mode (`ExitMeta::vb`).
fn vb_loc() -> rn_jit::tr::ra::Loc {
    match vb_reg() {
        Some(r) => rn_jit::tr::ra::Loc::G(r),
        None => asm::VB,
    }
}

/// Runs `f` once per budget mode: the memory form, then the register form (`TR_ASM_VB=mem|reg` selects one: the
/// mutant child processes use it to show that each form catches the budget mutants on its own).
fn both(f: impl Fn()) {
    let modes: Vec<Option<u8>> = match std::env::var("TR_ASM_VB").as_deref() {
        Ok("mem") => vec![None],
        Ok("reg") => vec![Some(ra::BUDGET_GPR)],
        _ => vec![None, Some(ra::BUDGET_GPR)],
    };
    for m in modes {
        VB_REG.with(|v| v.set(m));
        f();
    }
    VB_REG.with(|v| v.set(None));
}

// ---- synthetic memory --------------------------------------------------------------------------------------------

/// The budget word followed by the exit area, as `r15` addresses them (5.6).
#[repr(C)]
struct Bud {
    cnt: i32,
    _pad: u32,
    area: ExitArea,
    /// P2-3c: the float-constant pool behind the area (as in `rt::BudgetHdr`).
    fpool: FPool,
    /// P2-3b: the poll word (`irq`) lives in reach of `r15`.
    irq: [u8; 8],
}

/// The poll word of a `Bud`, seen as a slice (`w.irq[0] = 1`).
struct Irq(*mut [u8; 8]);

impl std::ops::Deref for Irq {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        // SAFETY: points into the live `Bud` owned by the `World` that owns this view.
        unsafe { &*self.0 }
    }
}

impl std::ops::DerefMut for Irq {
    fn deref_mut(&mut self) -> &mut [u8] {
        // SAFETY: as above; the world is borrowed mutably.
        unsafe { &mut *self.0 }
    }
}

/// Slots below the root base (the function slot is the last one).
const PRE: usize = 2;
/// Slots above the base the frame buffer has.
const NSLOT: usize = 16;
/// Slots of the model (the generator's frame).
const MODEL_SLOTS: usize = 8;
const ASIZE: usize = 8;
const NUPV: usize = 6;

/// A synthetic table: the 48-byte header and the array part (payloads below `arr`, tags above, T1).
struct Tbl {
    hdr: Vec<u64>,
    arr: Vec<u64>,
    /// The node vector (F1): `1 << lsizenode` nodes of 24 bytes.
    nodes: Vec<u64>,
}

impl Tbl {
    fn new(n: usize, nnodes: usize) -> Tbl {
        Tbl {
            hdr: vec![0; table::SIZE / 8],
            arr: vec![0; n + 1 + (n + 4).div_ceil(8) + 1],
            nodes: vec![0; nnodes * node::SIZE / 8],
        }
    }
    fn arr_ptr(&mut self, n: usize) -> *mut u8 {
        // the array pointer addresses the u32 length hint; payload k at arr - 8(k+1), tag k at arr + 4 + k
        self.arr.as_mut_ptr().cast::<u8>().wrapping_add(8 * n)
    }
}

struct Clo {
    mem: Vec<u64>,
    upvals: Vec<Vec<u64>>,
    cells: Vec<Vec<u64>>,
}

struct World {
    /// `PRE + NSLOT` slots of 16 bytes.
    frame: Vec<u64>,
    ci: Vec<u64>,
    thread: Vec<u64>,
    k: Vec<u64>,
    code: Vec<u32>,
    bud: *mut Bud,
    tabs: Vec<Tbl>,
    clo: Clo,
    irq: Irq,
    counters: Vec<u32>,
}

impl Drop for World {
    fn drop(&mut self) {
        // SAFETY: `bud` came from `Box::into_raw` in `World::new` and is freed once.
        unsafe { drop(Box::from_raw(self.bud)) };
    }
}

fn put<T: Copy>(buf: &mut [u64], off: usize, v: T) {
    assert!(off + std::mem::size_of::<T>() <= buf.len() * 8);
    // SAFETY: bounds checked; unaligned write inside the buffer.
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
    // SAFETY: bounds checked; unaligned read inside the buffer.
    unsafe {
        buf.as_ptr()
            .cast::<u8>()
            .add(off)
            .cast::<T>()
            .read_unaligned()
    }
}

impl World {
    fn new(shape: &Model) -> World {
        let bud = Box::into_raw(Box::new(Bud {
            cnt: 0,
            _pad: 0,
            area: ExitArea::default(),
            fpool: FPool::new(),
            irq: [0; 8],
        }));
        let mut w = World {
            frame: vec![0; (PRE + NSLOT) * 2],
            ci: vec![0; ci::SIZE.div_ceil(8) + 1],
            thread: vec![0; 128],
            k: vec![0; 8],
            code: vec![0; 64],
            bud,
            tabs: shape
                .tables
                .iter()
                .map(|t| Tbl::new(t.array.len(), 1 << t.lsizenode))
                .collect(),
            clo: Clo {
                mem: vec![0; (lclosure::UPVALS + 8 * NUPV).div_ceil(8) + 1],
                upvals: (0..NUPV).map(|_| vec![0; 8]).collect(),
                cells: (0..NUPV).map(|_| vec![0; 2]).collect(),
            },
            // SAFETY: `bud` was just allocated; only the address of the field is formed.
            irq: Irq(unsafe { std::ptr::addr_of_mut!((*bud).irq) }),
            counters: vec![0; 600],
        };
        // UpVal objects and the closure's upvalue pointers never change
        for j in 0..NUPV {
            let cell = w.clo.cells[j].as_mut_ptr() as usize;
            put(&mut w.clo.upvals[j], upval::V, cell);
            let uv = w.clo.upvals[j].as_mut_ptr() as usize;
            put(&mut w.clo.mem, lclosure::UPVALS + 8 * j, uv);
        }
        w
    }

    fn rbx(&mut self) -> *mut u8 {
        self.frame.as_mut_ptr().cast::<u8>().wrapping_add(PRE * 16)
    }

    fn tab_addr(&mut self, h: u64) -> u64 {
        self.tabs[h as usize - 1].hdr.as_mut_ptr() as u64
    }

    fn clo_addr(&mut self) -> u64 {
        self.clo.mem.as_mut_ptr() as u64
    }

    fn handle_of(&mut self, addr: u64) -> Option<u64> {
        (1..=self.tabs.len() as u64).find(|&h| self.tab_addr(h) == addr)
    }

    fn set_cnt(&mut self, v: i32) {
        // SAFETY: `bud` is live (owned by `self`).
        unsafe { (*self.bud).cnt = v };
    }

    fn set_spill(&mut self, slot: usize, v: u64) {
        // SAFETY: `bud` is live (owned by `self`); `slot < NSPILL` is checked by the index.
        unsafe { (*self.bud).area.spill[slot] = v };
    }

    fn cnt(&self) -> i32 {
        // SAFETY: `bud` is live.
        unsafe { (*self.bud).cnt }
    }

    /// The `CallInfo`: `func` = the function slot index, trap, `kcache`.
    fn setup_ci(&mut self, trap: i32) {
        put(&mut self.ci, ci::FUNC, PRE - 1);
        put(&mut self.ci, ci::TOP, PRE + NSLOT);
        put(&mut self.ci, ci::TRAP, trap);
        put(&mut self.ci, ci::KCACHE, self.k.as_ptr() as usize);
        put(&mut self.ci, ci::NEXT, 0usize);
        put(&mut self.ci, ci::PREVIOUS, 0usize);
        let c = self.ci.as_ptr() as usize;
        put(&mut self.thread, thr::CI, c);
        put(&mut self.thread, thr::STACK_SIZE, 100_000usize);
        put(&mut self.thread, thr::TOP, PRE + NSLOT);
    }

    /// Loads a model into the memory. Returns nothing; use [`World::uv_rel`] for the model's `uv_rel`.
    fn load(&mut self, m: &Model) {
        for w in self.frame.iter_mut() {
            *w = 0;
        }
        let clo = self.clo_addr();
        self.frame[(PRE - 1) * 2] = u64::from(ctb(LUA_VLCL));
        self.frame[(PRE - 1) * 2 + 1] = clo;
        for (i, s) in m.slots.iter().enumerate() {
            let (tag, mut bits) = tp(*s);
            if s.t == Ty::Tab {
                bits = self.tab_addr(s.bits);
            }
            self.frame[(PRE + i) * 2] = tag;
            self.frame[(PRE + i) * 2 + 1] = bits;
        }
        for (h, t) in m.tables.iter().enumerate() {
            let n = t.array.len();
            let arr = self.tabs[h].arr_ptr(n) as usize;
            let tb = &mut self.tabs[h];
            for w in tb.hdr.iter_mut() {
                *w = 0;
            }
            for w in tb.arr.iter_mut() {
                *w = 0;
            }
            put(&mut tb.hdr, table::FLAGS, t.flags);
            put(&mut tb.hdr, table::LSIZENODE, t.lsizenode);
            put(&mut tb.hdr, table::ASIZE, n as u32);
            put(&mut tb.hdr, table::ARRAY, if n == 0 { 0 } else { arr });
            put(&mut tb.hdr, table::METATABLE, t.metatable as usize);
            // the node vector: absent nodes carry a dead key (never equal to a short string)
            for w in tb.nodes.iter_mut() {
                *w = 0;
            }
            let nbase = tb.nodes.as_mut_ptr() as usize;
            put(&mut tb.hdr, table::NODE, nbase);
            for j in 0..(1usize << t.lsizenode) {
                let o = j * node::SIZE;
                match t.nodes.get(&(j as u32)) {
                    Some(nd) => {
                        let (tag, bits) = tp(nd.val);
                        put(&mut tb.nodes, o + node::VAL, bits);
                        put(&mut tb.nodes, o + node::VAL_TT, tag as u8);
                        put(&mut tb.nodes, o + node::KEY_TT, ctb(LUA_VSHRSTR));
                        put(&mut tb.nodes, o + node::KEY, nd.key);
                        put(&mut tb.nodes, o + node::NEXT, nd.next);
                    }
                    None => put(&mut tb.nodes, o + node::KEY_TT, LUA_TDEADKEY),
                }
            }
            if n > 0 {
                // SAFETY: `arr` addresses the buffer's length-hint word (sized in `Tbl::new`); 5.4: the array limit.
                unsafe { (arr as *mut u32).write_unaligned(t.lenhint) };
            }
            for (k, e) in t.array.iter().enumerate() {
                let (tag, bits) = tp(*e);
                // SAFETY: `arr` addresses the buffer's length-hint word; element `k` is inside it (sized in `Tbl::new`).
                unsafe {
                    (arr as *mut u8)
                        .add(table::ARRAY_TAG_BASE + k)
                        .write(tag as u8);
                    (arr as *mut u8)
                        .sub(8 * (k + 1))
                        .cast::<u64>()
                        .write_unaligned(bits);
                }
            }
        }
        for (j, v) in m.closures.first().into_iter().flatten().enumerate() {
            let (tag, bits) = tp(*v);
            self.clo.cells[j][0] = tag;
            self.clo.cells[j][1] = bits;
        }
        // SAFETY: `bud` is live.
        unsafe {
            (*self.bud).cnt = m.budget as i32;
            (*self.bud).area = ExitArea::default();
        }
        for c in self.counters.iter_mut() {
            *c = 0;
        }
        self.irq.iter_mut().for_each(|b| *b = 0);
    }

    /// `uv.v - rbx` of every upvalue of the closure.
    fn uv_rel(&mut self) -> Vec<Vec<u64>> {
        let rbx = self.rbx() as u64;
        vec![(0..NUPV)
            .map(|j| (self.clo.cells[j].as_ptr() as u64).wrapping_sub(rbx))
            .collect()]
    }

    fn env(&mut self) -> Env {
        Env {
            frame: self.rbx(),
            ci: self.ci.as_mut_ptr().cast(),
            thread: self.thread.as_mut_ptr().cast(),
            k: self.k.as_ptr().cast(),
            cnt: self.bud.cast::<i32>(),
        }
    }

    /// The observable state, normalised (table pointers -> handles) for comparison with the model.
    fn dump(&mut self) -> Dump {
        let mut frame = Vec::new();
        for i in 0..NSLOT {
            let (t, mut p) = (self.frame[(PRE + i) * 2], self.frame[(PRE + i) * 2 + 1]);
            if t == u64::from(ctb(LUA_VTABLE)) {
                p = self.handle_of(p).unwrap_or(p);
            }
            frame.push((t, p));
        }
        let mut arrays = Vec::new();
        let mut hints = Vec::new();
        for h in 0..self.tabs.len() {
            let n = get::<u32>(&self.tabs[h].hdr, table::ASIZE) as usize;
            let arr = get::<usize>(&self.tabs[h].hdr, table::ARRAY);
            hints.push(if n > 0 {
                // SAFETY: a table with `n > 0` elements has the length-hint word at `arr` (see `load`).
                unsafe { (arr as *const u32).read_unaligned() }
            } else {
                0
            });
            let mut v = Vec::new();
            for k in 0..n {
                // SAFETY: element `k` of a table loaded by `load`.
                let (t, p) = unsafe {
                    (
                        u64::from((arr as *const u8).add(table::ARRAY_TAG_BASE + k).read()),
                        (arr as *const u8)
                            .sub(8 * (k + 1))
                            .cast::<u64>()
                            .read_unaligned(),
                    )
                };
                v.push((t, p));
            }
            arrays.push(v);
        }
        let cells = (0..NUPV)
            .map(|j| (self.clo.cells[j][0], self.clo.cells[j][1]))
            .collect();
        // the value of every node (tag, payload; the key words must not change either)
        let hashes = (0..self.tabs.len())
            .map(|h| {
                let n = 1usize << get::<u8>(&self.tabs[h].hdr, table::LSIZENODE);
                (0..n)
                    .map(|j| {
                        let o = j * node::SIZE;
                        let tb = &self.tabs[h].nodes;
                        let tag = get::<u8>(tb, o + node::VAL_TT);
                        let bits = get::<u64>(tb, o + node::VAL);
                        // a nil keeps no payload meaning
                        let bits = if tag & 0x0F == 0 { 0 } else { bits };
                        (u64::from(tag), bits, get::<u64>(tb, o + node::KEY))
                    })
                    .collect()
            })
            .collect();
        Dump {
            frame,
            cnt: self.cnt(),
            arrays,
            hints,
            cells,
            hashes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Dump {
    frame: Vec<(u64, u64)>,
    cnt: i32,
    arrays: Vec<Vec<(u64, u64)>>,
    /// The array limit word of each table (5.4: `ATOUCH` updates it; 5.5: always 0).
    hints: Vec<u32>,
    cells: Vec<(u64, u64)>,
    /// F1: per table and node, `(value tag, value payload, key payload)`; absent nodes are all zero.
    hashes: Vec<Vec<(u64, u64, u64)>>,
}

fn tag_of_val(v: Val) -> u64 {
    exit::tag_of(v.t).map_or(0, u64::from)
}

fn tp(v: Val) -> (u64, u64) {
    (tag_of_val(v), v.bits)
}

/// The dump the model predicts after a run that ended in an exit (`slots` are the restored snapshot entries).
fn expected_dump(m: &Model, slots: &[(u16, u8, Val)], refund: i32) -> Dump {
    let mut frame: Vec<(u64, u64)> = vec![(0, 0); NSLOT];
    for (i, s) in m.slots.iter().enumerate() {
        frame[i] = tp(*s);
    }
    for &(slot, _, v) in slots {
        frame[usize::from(slot)] = tp(v);
    }
    let arrays = m
        .tables
        .iter()
        .map(|t| t.array.iter().map(|&e| tp(e)).collect())
        .collect();
    let hints = m
        .tables
        .iter()
        .map(|t| if t.array.is_empty() { 0 } else { t.lenhint })
        .collect();
    let cells = m
        .closures
        .first()
        .map(|c| c.iter().map(|&v| tp(v)).collect())
        .unwrap_or_default();
    let hashes = m
        .tables
        .iter()
        .map(|t| {
            (0..1u32 << t.lsizenode)
                .map(|j| match t.nodes.get(&j) {
                    Some(nd) => {
                        let (tag, bits) = tp(nd.val);
                        (tag, if tag & 0x0F == 0 { 0 } else { bits }, nd.key)
                    }
                    None => (0, 0, 0),
                })
                .collect()
        })
        .collect();
    Dump {
        frame,
        cnt: (m.budget + i64::from(refund)) as i32,
        arrays,
        hints,
        cells,
        hashes,
    }
}

// ---- the emulator backend ---------------------------------------------------------------------------------------

const EXIT_FN: u64 = 0xE000_0001;
const CALLN_FN: u64 = 0xE000_0002;
const BARRIER_FN: u64 = 0xE000_0003;
const ENTRY_FAIL_FN: u64 = 0xE000_0004;
const MAT_FN: u64 = 0xE000_0005;
const LAND_BASE: u64 = 0x5000_0000;
const PCL_H: u64 = 0x6000_0000;

#[derive(Default)]
struct Log {
    barriers: Vec<(u64, u64, u32)>,
    /// The `b` word of every barrier call (BUNDLE-V: 1 = the barrier of an upvalue).
    barrier_bs: Vec<u32>,
    entry_fail_calls: u32,
    exit_calls: u32,
    /// Handler calls whose `exitno` carried `exit::EXIT_ENTRY` (the generic twin of an entry check).
    exit_entry_calls: u32,
    /// Handler calls whose `exitno` carried `exit::EXIT_POLL` (P-L: the poll exits).
    exit_poll_calls: u32,
    /// J1-2: calls of the materialising helper (test double) with their `(snap, trace_id)` and the exit area's register words they saw.
    mat_calls: Vec<(u32, u32, Vec<u64>)>,
    /// J1-2: the test double fails (returns 1).
    mat_fail: bool,
}

/// The helper model of the emulator: the exit handler (`exit::restore`), the pure helpers, the barrier recorder.
struct Hlp<'a> {
    snaps: &'a [ExitSnap],
    stack_size: usize,
    stack0: *mut u8,
    root: *mut CallInfo,
    code: *const u32,
    log: &'a RefCell<Log>,
}

impl Helpers for Hlp<'_> {
    unsafe fn call(&mut self, f: u64, a: u32, _b: u32, env: &Env, _frame: *mut u8) -> HelperOut {
        // SAFETY: `env.cnt` is the budget word of a `Bud` (provenance over the whole box).
        let area = unsafe { &mut *env.cnt.cast::<u8>().add(8).cast::<ExitArea>() };
        match f {
            EXIT_FN => {
                self.log.borrow_mut().exit_calls += 1;
                if area.exitno & exit::EXIT_ENTRY != 0 {
                    self.log.borrow_mut().exit_entry_calls += 1;
                }
                if area.exitno & exit::EXIT_POLL != 0 {
                    self.log.borrow_mut().exit_poll_calls += 1;
                }
                let s = &self.snaps[(area.exitno & !exit::EXIT_FLAGS) as usize];
                let ctx = exit::RestoreCtx {
                    stack0: self.stack0,
                    stack_size: self.stack_size,
                    root: self.root,
                    // SAFETY: inside the thread buffer.
                    th_ci: unsafe { env.thread.add(thr::CI) }.cast(),
                    // SAFETY: as above.
                    th_top: unsafe { env.thread.add(thr::TOP) }.cast(),
                    ncalls: std::ptr::null_mut(),
                    budget: env.cnt,
                    hooked: false,
                };
                let area_copy = area.clone();
                // SAFETY: the context describes the synthetic frame; no inlined frames occur in these tests.
                area.args[0] = unsafe { exit::restore(&ctx, s, self.code, vb_loc(), &area_copy) };
            }
            CALLN_FN => area.args[0] = asm::calln_core(a, area.args[0], area.args[1]),
            BARRIER_FN => {
                let mut l = self.log.borrow_mut();
                l.barriers.push((area.args[0], area.args[1], a));
                l.barrier_bs.push(_b);
            }
            ENTRY_FAIL_FN => self.log.borrow_mut().entry_fail_calls += 1,
            MAT_FN => {
                let mut l = self.log.borrow_mut();
                let words: Vec<u64> = area.gpr.iter().chain(area.xmm.iter()).copied().collect();
                l.mat_calls.push((a, _b, words));
                return HelperOut {
                    result: u32::from(l.mat_fail),
                    new_frame: None,
                };
            }
            f => panic!("unexpected helper {f:#x}"),
        }
        HelperOut {
            result: 0,
            new_frame: None,
        }
    }
}

struct TestRes {
    /// Landing address of `pc` (`pc` = snapshot index in these tests).
    addrs: Vec<u64>,
}

impl TestRes {
    fn linear(n: usize) -> TestRes {
        TestRes {
            addrs: (0..n as u64).map(|i| LAND_BASE + 16 * i).collect(),
        }
    }
}

impl Resolver for TestRes {
    fn proto(&self, _key: ProtoKey) -> Option<ProtoInfoR> {
        Some(ProtoInfoR {
            code: std::ptr::null(),
            k: std::ptr::null(),
            maxstack: 16,
        })
    }
    fn land_addr(&self, _key: ProtoKey, pc: u32, _land: Land) -> Option<u64> {
        self.addrs.get(pc as usize).copied()
    }
}

/// P2 bundle 1: the allocatable extra registers of the tests (`rbp` = bit 0, `r14` = bit 1): both on; `TR_ASM_XREGS=0` runs the old register file (control).
fn xregs() -> u8 {
    std::env::var("TR_ASM_XREGS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3)
}

/// Assembles `ir` with the allocation of seed `ra_seed` (`None`: the default allocator).
fn compile(
    ir: &TraceIr,
    writeback: &[SnapEntry],
    ra_seed: Option<u64>,
    env: &AsmEnv,
    res: &TestRes,
) -> Result<(Asm, Allocation, Vec<ExitSnap>), String> {
    let end: Vec<Ref> = if ir.loop_at.is_none() {
        writeback.iter().map(|e| e.r).collect()
    } else {
        Vec::new()
    };
    let opts = RaOpts {
        randra: ra_seed,
        reserve_gpr: vb_reg(),
        xregs: xregs(),
        ..RaOpts::default()
    };
    let al = ra::allocate(ir, &end, &opts).map_err(|e| format!("ra: {e:?}"))?;
    let (snaps, _) = exit::build_exits(ir, &al, res)?;
    let a = asm::assemble(ir, &al, &snaps, writeback, env).map_err(|e| format!("asm: {e:?}"))?;
    Ok((a, al, snaps))
}

fn base_env(w: &mut World) -> AsmEnv {
    AsmEnv {
        seg_oracle: false,
        link_mat: false,
        trace_id: 0,
        counters: w.counters.as_ptr() as u64,
        hot_exit: 1 << 30,
        shared_exit: 0,
        exit_handler: EXIT_FN,
        stack_off: 0,
        pcl_h: PCL_H,
        head: 0,
        head_pc: 0,
        entry_fail_fn: ENTRY_FAIL_FN,
        calln: CALLN_FN,
        barrier: BARRIER_FN,
        alloc: 0,
        interrupt: w.irq.as_ptr() as u64,
        exitstorm: None,
        generic_only: false,
        count_iterations: false,
        budget_reg: vb_reg(),
        exit_cells: 0,
        side: None,
        vcell: 0,
        rec_sites: [(0, 0, 0); 6],
        rec_nsites: 0,
        rec_link_cell: 0,
        root_maxstack: 0,
        no_room_checks: false,
        side_room: false,
        frame_cells: false,
        lazy: false,
        prelude: false,
        lazy_leave_fn: 0,
        chain: false,
        root_code: 0,
        lazy_cs: None,
        lazy_nparams: 0,
        frame_link_snap: u32::MAX,
        setmt_inline: true,
        b2_arr: b2_knobs(),
        b2_tag: b2_knobs(),
        b2_hk: b2_knobs(),
        nopay: b4_knobs(),
        divmagic: b4_knobs(),
        hk_fuse: b4_knobs(),
        hk_key64: b4_knobs(),
        f2_byte: true,
        ccalls_inline: false,
        f2ifl: true,
        fibdiet: 0,
        lean_sub: true,
        libcall: 0,
        sink_stub: false,
        mat_fn: MAT_FN,
        budget_sub: ld_knobs(),
        poll_off: ld_knobs().then(|| (w.irq.as_ptr() as i64 - w.bud as i64) as i32),
        // SAFETY: only the address of the field of the live `Bud` is formed.
        fpool: if ld_knobs() {
            unsafe { std::ptr::addr_of_mut!((*w.bud).fpool) as u64 }
        } else {
            0
        },
        fpool_off: (exit_area::FROM_R15 as usize + exit_area::SIZE + 8) as i32,
    }
}

/// B2: the three backend knobs of `AsmEnv` (`b2arr`, `b2tag`, `b2hk`) are on in these tests; `TR_ASM_B2=0` runs the old forms (control).
fn b2_knobs() -> bool {
    std::env::var("TR_ASM_B2").map_or(true, |v| v != "0")
}

/// P2-3: the loop-top diet knobs of `AsmEnv` (`budget_sub`, `poll_off`, `fpool`) are on in these tests; `TR_ASM_LD=0` runs the old forms (control).
fn ld_knobs() -> bool {
    std::env::var("TR_ASM_LD").map_or(true, |v| v != "0")
}

/// B4: the BUNDLE-4 backend knobs of `AsmEnv` (`nopay`, `divmagic`, `hk_fuse`, `hk_key64`) are on in these tests; `TR_ASM_B4=0` runs the
/// old forms (control).
fn b4_knobs() -> bool {
    std::env::var("TR_ASM_B4").map_or(true, |v| v != "0")
}

/// Runs the assembled trace in the emulator until it leaves through a `JmpR`; returns the target.
fn run_emu(a: &Asm, snaps: &[ExitSnap], w: &mut World, log: &RefCell<Log>) -> u64 {
    let env = w.env();
    let mut hl = Hlp {
        snaps,
        stack_size: PRE + NSLOT,
        stack0: w.frame.as_mut_ptr().cast(),
        root: w.ci.as_mut_ptr().cast(),
        code: w.code.as_ptr(),
        log,
    };
    let mut emu = Emu::new(&a.prog, a.entry);
    // SAFETY: the buffers of `w` cover every offset the trace uses (frame, ci, thread, area, tables, closures).
    let u = unsafe { emu.run(&a.prog, &[], &env, 3_000_000, &mut hl) };
    assert!(matches!(u, Until::Stop(Stop::Unsupported)), "{u:?}");
    emu.jmp_target().expect("a JmpR target")
}

// ---- random IR vs the reference interpreter -----------------------------------------------------------------------

/// Rewrites model handles in the constants to real addresses: tables (`Ptr(h, Raw)`) and the closure
/// (`Ptr(1, LClosure)`).
fn rewrite_consts(ir: &mut TraceIr, w: &mut World) {
    for k in ir.k.iter_mut() {
        if let KVal::Ptr(h, kind) = *k {
            *k = match kind {
                // a hash key (F1: an address like 0x1000) is not a table handle
                PtrKind::Raw if (h as usize) <= w.tabs.len() => KVal::Ptr(w.tab_addr(h), kind),
                PtrKind::LClosure => KVal::Ptr(w.clo_addr(), kind),
                _ => *k,
            };
        }
    }
}

/// Makes every snapshot name its own landing address (`pc` = snapshot index) and gives it a random refund.
fn tag_snaps(ir: &mut TraceIr, rng: &mut Rng) {
    // P-L: the snapshot of a POLL is a loop head: its poll exit hands over to the interpreter at a pre-count label
    let polls: Vec<u32> = ir
        .ins
        .iter()
        .filter(|i| i.op == TOp::Poll)
        .map(|i| aux_snap(i.aux))
        .collect();
    for (i, s) in ir.snaps.iter_mut().enumerate() {
        s.pc = i as u32;
        s.refund = rng.below(9) as i32 - 3;
        s.land = if polls.contains(&(i as u32)) || rng.chance(1, 2) {
            Land::Pre
        } else {
            Land::Mid
        };
    }
}

/// A random writeback for a rolled iteration: values of the right type back into the slots that hold that type.
fn random_writeback(ir: &mut TraceIr, rng: &mut Rng) -> Vec<SnapEntry> {
    let ints: Vec<Ref> = (0..ir.ins.len())
        .filter(|&i| {
            let n = &ir.ins[i];
            n.t == Ty::Int
                && !matches!(
                    n.op,
                    TOp::Poll
                        | TOp::Budget
                        | TOp::Eq
                        | TOp::Ne
                        | TOp::Lt
                        | TOp::Le
                        | TOp::Gt
                        | TOp::Ge
                        | TOp::Ult
                        | TOp::Uge
                        | TOp::Fcmp
                        | TOp::Eqref
                        | TOp::Astore
                        | TOp::Ustore
                )
        })
        .map(|i| REF_BIAS + i as Ref)
        .collect();
    let nums: Vec<Ref> = (0..ir.ins.len())
        .filter(|&i| {
            ir.ins[i].t == Ty::Num && !matches!(ir.ins[i].op, TOp::Fcmp | TOp::Astore | TOp::Ustore)
        })
        .map(|i| REF_BIAS + i as Ref)
        .collect();
    let mut v = Vec::new();
    for slot in 0..5u16 {
        if rng.chance(2, 3) {
            let pool = if slot < 3 { &ints } else { &nums };
            if pool.is_empty() {
                continue;
            }
            let r = pool[rng.below(pool.len() as u64) as usize];
            v.push(SnapEntry { slot, r, flags: 0 });
        }
    }
    // an occasional constant
    if rng.chance(1, 4) {
        let k = ir.kint(rng.below(100) as i64);
        v.push(SnapEntry {
            slot: 7,
            r: k,
            flags: 0,
        });
    }
    v
}

/// The reference run of an iteration: the interpreter, with the writeback applied between rolled iterations.
/// Returns `(snapshot, restored slots, final model)`.
fn reference(
    ir: &TraceIr,
    writeback: &[SnapEntry],
    m0: &Model,
) -> (u32, Vec<(u16, u8, Val)>, Model) {
    let mut m = m0.clone();
    m.max_iters = 1_000_000;
    if ir.loop_at.is_some() {
        match interp::run(ir, &mut m).expect("valid IR") {
            Outcome::Exit { snap, slots } => return (snap, slots, m),
            Outcome::Done { .. } => panic!("the reference run ended without an exit"),
        }
    }
    // rolled: probe snapshot with the writeback entries and an always-failing guard
    let mut p = ir.clone();
    let probe_snap = p.snapshot(Snap::default(), writeback);
    let k0 = p.kint(0);
    let k1 = p.kint(1);
    p.emit(Ins {
        op: TOp::Eq,
        t: Ty::Int,
        flags: F_GUARD,
        a: k0,
        b: k1,
        prev: NOREF,
        aux: guard_aux(probe_snap, 0),
    });
    for _ in 0..1000 {
        match interp::run(&p, &mut m).expect("valid IR") {
            Outcome::Exit { snap, slots } if snap == probe_snap => {
                for (slot, _, v) in slots {
                    m.slots[usize::from(slot)] = v;
                }
            }
            Outcome::Exit { snap, slots } => return (snap, slots, m),
            Outcome::Done { .. } => panic!("probe did not fire"),
        }
    }
    panic!("the rolled reference loop did not terminate");
}

thread_local! {
    static STATS: RefCell<Stats> = RefCell::new(Stats::default());
}

#[derive(Default, Debug, Clone)]
struct Stats {
    cases: u32,
    runs: u32,
    exits_by_snap0: u32,
    loops: u32,
    generic_exits: u32,
    /// Guard ops that were the reason of an observed exit, and the ops that occurred in assembled IR.
    exit_ops: std::collections::BTreeSet<&'static str>,
    ops_seen: std::collections::BTreeSet<&'static str>,
}

fn model_for(w: &mut World, m: &Model) -> Model {
    let mut m = m.clone();
    // the model's frame has MODEL_SLOTS entries
    m.slots.resize(MODEL_SLOTS, Val::NIL);
    m.uv_rel = w.uv_rel();
    // trap at the first poll <=> `poll_exit == Some(1)`; later polls cannot be modelled by a flag
    if m.poll_exit != Some(1) {
        m.poll_exit = None;
    }
    m
}

/// Which backend runs the assembled trace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Backend {
    /// The Rust emulator (Miri-runnable) with inline stubs.
    Emu,
    /// The emulator with generic stubs only (the shared routine and `exit::restore` for every exit).
    EmuGeneric,
    /// `x86::emit` and the thunk (inline stubs).
    #[cfg(not(miri))]
    Native,
}

/// Does the assembler call `guard_label` for this instruction (the `+exitstorm` check is emitted for it)?
fn is_storm_guard(i: &Ins) -> bool {
    match i.op {
        TOp::Sload | TOp::Aload | TOp::Uload | TOp::Hload => aux_bits(i.aux) & 1 != 0,
        TOp::Hrefk | TOp::Hmiss => true,
        TOp::Hstore => i.flags & F_GUARD != 0 && aux_bits(i.aux) & 1 != 0,
        TOp::Idiv | TOp::Mod => i.flags & F_GUARD != 0,
        op => op.always_guard() || op == TOp::F2iExact,
    }
}

/// The number of guards the storm counts in one pass of `ir`.
fn storm_guards(ir: &TraceIr) -> usize {
    ir.ins.iter().filter(|i| is_storm_guard(i)).count()
}

/// The IR with its `n`-th guard (1-based, in IR order) replaced by an always-failing one (same snapshot): the
/// reference of `+exitstorm` when the storm counter is set up to fire at the `n`-th guard execution.
fn force_nth_guard(ir: &TraceIr, n: usize) -> TraceIr {
    let mut f = ir.clone();
    let g = (0..f.ins.len())
        .filter(|&i| is_storm_guard(&f.ins[i]))
        .nth(n - 1)
        .expect("a guard");
    let (k0, k1) = (f.kint(0), f.kint(1));
    let aux = guard_aux(aux_snap(f.ins[g].aux), 0);
    f.ins[g] = Ins {
        op: TOp::Eq,
        t: Ty::Int,
        flags: F_GUARD,
        a: k0,
        b: k1,
        prev: NOREF,
        aux,
    };
    f
}

#[cfg(not(miri))]
mod native {
    use super::*;
    use rn_jit::ir::{Op, Program};
    use rn_jit::mem::CodeArena;
    use rn_jit::x86::{emit, thunk_code, Thunk};
    use std::sync::Mutex;

    pub static BARRIERS: Mutex<Vec<(u64, u64, u32)>> = Mutex::new(Vec::new());
    /// Where the fake helpers find the exit area: a pointer stored at the end of the fake thread.
    pub const AREA_AT: usize = 127 * 8;

    /// # Safety
    /// Called from generated code with the test's fake thread.
    pub unsafe extern "C" fn calln(th: *mut u8, _ci: *mut u8, which: u32, _b: u32) -> u32 {
        // SAFETY: the test stores the area pointer at `AREA_AT` of its thread buffer.
        unsafe {
            let area = *(th.add(AREA_AT) as *const *mut ExitArea);
            (*area).args[0] = asm::calln_core(which, (*area).args[0], (*area).args[1]);
        }
        0
    }

    /// # Safety
    /// As [`calln`].
    pub unsafe extern "C" fn barrier(th: *mut u8, _ci: *mut u8, tag: u32, _b: u32) -> u32 {
        // SAFETY: as in `calln`.
        unsafe {
            let area = *(th.add(AREA_AT) as *const *mut ExitArea);
            BARRIERS
                .lock()
                .unwrap()
                .push(((*area).args[0], (*area).args[1], tag));
        }
        0
    }

    /// What the native exit handler needs (P-L: the poll exits are generic stubs, so the native runs call a handler too).
    pub struct NCtx {
        pub snaps: Vec<ExitSnap>,
        pub stack0: *mut u8,
        pub stack_size: usize,
        pub root: *mut CallInfo,
        pub code: *const u32,
        pub cnt: *mut i32,
    }

    thread_local! {
        pub static CTX: std::cell::RefCell<Option<NCtx>> = const { std::cell::RefCell::new(None) };
    }

    /// The exit handler of the native runs: `exit::restore` of the snapshot, landing address in `args[0]` (the model of
    /// `Hlp` in the emulator runs).
    ///
    /// # Safety
    /// Called from generated code with the test's fake thread; [`CTX`] was set by [`Native::run`].
    pub unsafe extern "C" fn exit(th: *mut u8, _ci: *mut u8, _a: u32, _b: u32) -> u32 {
        // SAFETY: the test stores the area pointer at `AREA_AT` of its thread buffer.
        unsafe {
            let area = *(th.add(AREA_AT) as *const *mut ExitArea);
            CTX.with(|c| {
                let c = c.borrow();
                let c = c.as_ref().expect("native exit context");
                let s = &c.snaps[((*area).exitno & !exit::EXIT_FLAGS) as usize];
                let ctx = exit::RestoreCtx {
                    stack0: c.stack0,
                    stack_size: c.stack_size,
                    root: c.root,
                    th_ci: th.add(thr::CI).cast(),
                    th_top: th.add(thr::TOP).cast(),
                    ncalls: std::ptr::null_mut(),
                    budget: c.cnt,
                    hooked: false,
                };
                let area_copy = (*area).clone();
                (*area).args[0] = exit::restore(&ctx, s, c.code, vb_loc(), &area_copy);
            });
        }
        0
    }

    pub struct Native {
        pub arena: CodeArena,
        pub res: TestRes,
    }

    impl Native {
        /// The landing block first (its addresses are embedded in the trace): `Ret(s + 1)` for every snapshot.
        pub fn new(nsnaps: usize) -> Native {
            let mut arena = CodeArena::new();
            let mut p = Program::default();
            let ls: Vec<_> = (0..nsnaps).map(|_| p.label()).collect();
            for (s, &l) in ls.iter().enumerate() {
                p.push(Op::Bind(l));
                p.push(Op::Ret(s as u32 + 1));
            }
            let em = emit(&p).expect("landing block");
            let mut b = arena.alloc(em.code.len(), 16).unwrap();
            arena.writer(&mut b).unwrap().write_at(0, &em.code);
            arena.commit().unwrap();
            let addrs = ls
                .iter()
                .map(|&l| (b.addr() + em.label_off[l as usize].unwrap()) as u64)
                .collect();
            Native {
                arena,
                res: TestRes { addrs },
            }
        }

        /// Places and runs the trace; the result is the landing code (`snapshot + 1`).
        pub fn run(&mut self, a: &Asm, snaps: &[ExitSnap], w: &mut World) -> u32 {
            CTX.with(|c| {
                *c.borrow_mut() = Some(NCtx {
                    snaps: snaps.to_vec(),
                    stack0: w.frame.as_mut_ptr().cast(),
                    stack_size: PRE + NSLOT,
                    root: w.ci.as_mut_ptr().cast(),
                    code: w.code.as_ptr(),
                    cnt: w.bud.cast::<i32>(),
                })
            });
            let em = emit(&a.prog).expect("assembles");
            let off = em.label_off[a.entry as usize].expect("entry bound");
            let tc = thunk_code();
            let mut tb = self.arena.alloc(tc.len(), 16).unwrap();
            self.arena.writer(&mut tb).unwrap().write_at(0, &tc);
            let mut cb = self.arena.alloc(em.code.len(), 16).unwrap();
            self.arena.writer(&mut cb).unwrap().write_at(0, &em.code);
            self.arena.commit().unwrap();
            // SAFETY: `tb` holds the thunk (committed); `Thunk` is its signature.
            let thunk: Thunk = unsafe { self.arena.entry(&tb) }.unwrap();
            let env = w.env();
            // SAFETY: `bud` is live; only the address is taken.
            let area = unsafe { &raw mut (*w.bud).area } as usize;
            put(&mut w.thread, AREA_AT, area);
            let env = Env {
                thread: w.thread.as_mut_ptr().cast(),
                ..env
            };
            // SAFETY: the buffers of `w` cover every offset the trace uses; the target is the bound entry of the
            // committed block; exits land in the committed landing block.
            unsafe {
                thunk(
                    env.thread,
                    env.ci,
                    env.frame,
                    env.cnt,
                    (cb.addr() + off) as *const u8,
                )
            }
        }
    }
}

/// One differential case: `ir` (rolled or peeled), every model, one allocation seed, one backend. `storm`:
/// `+exitstorm=N` (a random guard of the first pass fires: see `k`).
fn diff_case(
    ir0: &TraceIr,
    models: &[Model],
    seed: u64,
    ra_seed: Option<u64>,
    backend: Backend,
    storm: Option<u32>,
) {
    let mut rng = Rng::new(seed);
    let mut w = World::new(&models[0]);
    // `ir` is the model's IR (handles in the constants: the reference); `ira` the one with real addresses
    let mut ir = ir0.clone();
    tag_snaps(&mut ir, &mut rng);
    let writeback = if ir.loop_at.is_none() {
        random_writeback(&mut ir, &mut rng)
    } else {
        Vec::new()
    };
    let mut ira = ir.clone();
    rewrite_consts(&mut ira, &mut w);
    let mut env = base_env(&mut w);
    env.generic_only = backend == Backend::EmuGeneric;
    env.exitstorm = storm;
    #[cfg(not(miri))]
    let mut nat = (backend == Backend::Native).then(|| {
        env.calln = native::calln as *const () as usize as u64;
        env.barrier = native::barrier as *const () as usize as u64;
        env.exit_handler = native::exit as *const () as usize as u64;
        native::Native::new(ir.snaps.len())
    });
    #[cfg(not(miri))]
    let res = match &nat {
        Some(n) => TestRes {
            addrs: n.res.addrs.clone(),
        },
        None => TestRes::linear(ir.snaps.len()),
    };
    #[cfg(miri)]
    let res = TestRes::linear(ir.snaps.len());
    let (a, _al, snaps) = match compile(&ira, &writeback, ra_seed, &env, &res) {
        Ok(x) => x,
        Err(e) => panic!("{e}\n{}", rn_jit::tr::print::dump(&ira)),
    };
    // `+exitstorm`: the counter slot starts at `k`, so the k-th guard execution fires (the first pass executes the
    // guards in IR order); the reference is the IR with that guard forced to fail
    let k = if storm.is_some() {
        1 + rng.below(storm_guards(&ir).min(14) as u64) as usize
    } else {
        0
    };
    let ir_ref = if storm.is_some() {
        force_nth_guard(&ir, k)
    } else {
        ir.clone()
    };
    for (mi, m0) in models.iter().enumerate() {
        w.load(m0);
        let m = model_for(&mut w, m0);
        w.setup_ci(0);
        w.irq[0] = u8::from(m.poll_exit == Some(1)); // (P-L: the one poll word)
        if storm.is_some() {
            // the extension slot 1 is the storm counter (`asm::EXT_SLOTS`)
            w.set_spill(usize::from(_al.nspill) + 1, k as u64);
        }
        let (snap, slots, mfin) = reference(&ir_ref, &writeback, &m);
        let log = RefCell::new(Log::default());
        let got = match backend {
            Backend::Emu | Backend::EmuGeneric => {
                let target = run_emu(&a, &snaps, &mut w, &log);
                res.addrs
                    .iter()
                    .position(|&x| x == target)
                    .expect("a landing address") as u32
            }
            #[cfg(not(miri))]
            Backend::Native => nat.as_mut().expect("native").run(&a, &snaps, &mut w) - 1,
        };
        let ctx = format!("case seed {seed} ra {ra_seed:?} {backend:?} storm {storm:?} model {mi}");
        assert_eq!(
            got,
            snap,
            "{ctx}: exit snapshot\n{}",
            rn_jit::tr::print::dump(&ir)
        );
        let want = expected_dump(&mfin, &slots, ir.snaps[snap as usize].refund);
        let have = w.dump();
        assert_eq!(
            have.frame,
            want.frame,
            "{ctx}: frame\n{}",
            rn_jit::tr::print::dump(&ir)
        );
        assert_eq!(have.cnt, want.cnt, "{ctx}: budget word");
        assert_eq!(have.arrays, want.arrays, "{ctx}: arrays");
        assert_eq!(have.hints, want.hints, "{ctx}: array limits (ATOUCH)");
        assert_eq!(have.cells, want.cells, "{ctx}: upvalue cells");
        assert_eq!(have.hashes, want.hashes, "{ctx}: hash nodes");
        STATS.with(|s| {
            let mut s = s.borrow_mut();
            s.runs += 1;
            s.loops += u32::from(ir.loop_at.is_some());
            s.generic_exits += log.borrow().exit_calls;
            s.exits_by_snap0 += u32::from(snap == 0);
            for i in &ir.ins {
                s.ops_seen.insert(i.op.name());
                if i.flags & F_GUARD != 0 && aux_snap(i.aux) == snap {
                    s.exit_ops.insert(i.op.name());
                }
            }
        });
    }
    STATS.with(|s| s.borrow_mut().cases += 1);
}

fn gen_cases(n: u64, base: u64, cfg: GenCfg) -> Vec<(u64, TraceIr, Vec<Model>)> {
    (0..n)
        .map(|i| {
            let seed = base + i;
            let mut rng = Rng::new(seed);
            let c = random_case(&mut rng, cfg);
            (seed, c.ir, c.models)
        })
        .collect()
}

fn n_cases(small: u64, big: u64) -> u64 {
    if cfg!(miri) {
        small
    } else {
        big
    }
}

#[test]
fn emu_peeled_matches_interp() {
    both(|| {
        let mut loops = 0;
        for (seed, ir, models) in gen_cases(
            n_cases(3, 300),
            1000,
            GenCfg {
                size: 24,
                loops: true,
            },
        ) {
            if ir.loop_at.is_none() {
                continue;
            }
            loops += 1;
            for (k, b) in [
                (None, Backend::Emu),
                (Some(seed), Backend::Emu),
                (Some(seed + 7), Backend::EmuGeneric),
            ] {
                diff_case(&ir, &models, seed, k, b, None);
            }
        }
        assert!(loops > 0);
        let st = STATS.with(|s| s.borrow().clone());
        eprintln!("peeled stats: {st:?}");
    });
}

#[test]
fn emu_rolled_matches_interp() {
    both(|| {
        let mut n = 0;
        for (seed, ir, models) in gen_cases(
            n_cases(3, 300),
            5000,
            GenCfg {
                size: 24,
                loops: false,
            },
        ) {
            if ir.loop_at.is_some() {
                continue;
            }
            n += 1;
            for (k, b) in [
                (None, Backend::Emu),
                (Some(seed), Backend::Emu),
                (Some(seed + 7), Backend::EmuGeneric),
            ] {
                diff_case(&ir, &models, seed, k, b, None);
            }
        }
        assert!(n > 0);
        let st = STATS.with(|s| s.borrow().clone());
        eprintln!("rolled stats: {st:?}");
    });
}

#[test]
fn emu_exitstorm_forces_the_first_guard() {
    both(|| {
        for (loops, base) in [(true, 9000u64), (false, 9500)] {
            for (seed, ir, models) in gen_cases(n_cases(2, 120), base, GenCfg { size: 20, loops }) {
                if ir.loop_at.is_some() != loops {
                    continue;
                }
                for (n, b) in [(1, Backend::Emu), (5, Backend::EmuGeneric)] {
                    diff_case(&ir, &models, seed, Some(seed), b, Some(n));
                }
            }
        }
    });
}

/// F1: the random traces contain the hash ops (`HREFK` / `HLOAD` / `HSTORE`) and every one of them is the exit of some
/// run (a key that is not at the recorded node, a value of another type, a nil old value), so the differential
/// above is a differential of the hash assembly, not only of the ops around it.
#[test]
fn random_traces_cover_the_hash_ops() {
    both(|| {
        for (seed, ir, models) in gen_cases(
            n_cases(6, 250),
            31_000,
            GenCfg {
                size: 24,
                loops: true,
            },
        ) {
            diff_case(&ir, &models, seed, None, Backend::Emu, None);
            diff_case(&ir, &models, seed, Some(seed), Backend::Emu, None);
        }
        let st = STATS.with(|s| s.borrow().clone());
        for op in ["HREFK", "HLOAD", "HSTORE", "HMISS"] {
            assert!(st.ops_seen.contains(op), "{op} never generated: {st:?}");
            if !cfg!(miri) {
                assert!(st.exit_ops.contains(op), "{op} never exited: {st:?}");
            }
        }
    });
}

#[cfg(not(miri))]
#[test]
fn native_matches_the_reference_peeled_and_rolled() {
    both(|| {
        for (loops, base) in [(true, 20_000u64), (false, 21_000)] {
            let mut n = 0;
            for (seed, ir, models) in gen_cases(250, base, GenCfg { size: 24, loops }) {
                if ir.loop_at.is_some() != loops {
                    continue;
                }
                n += 1;
                diff_case(&ir, &models, seed, None, Backend::Native, None);
                diff_case(&ir, &models, seed, Some(seed ^ 0x55), Backend::Native, None);
                diff_case(&ir, &models, seed, Some(seed), Backend::Native, Some(3));
            }
            assert!(n > 50, "{n} cases");
        }
    });
}

// ---- hand-built IR -----------------------------------------------------------------------------------------------

/// A tiny IR builder for the hand tests (the P2 builder folds; these tests want the instructions as written).
struct B {
    ir: TraceIr,
}

impl B {
    fn new() -> B {
        let ir = TraceIr {
            head: (ProtoKey(1), 0),
            ..TraceIr::default()
        };
        B { ir }
    }
    /// A snapshot (`pc` = its index, as the landing addresses of the tests expect) with the given map.
    fn snap(&mut self, refund: i32, entries: &[SnapEntry]) -> u32 {
        let pc = self.ir.snaps.len() as u32;
        self.ir.snapshot(
            Snap {
                pc,
                refund,
                ..Snap::default()
            },
            entries,
        )
    }
    fn ins(&mut self, op: TOp, t: Ty, flags: u8, a: Ref, b: Ref, aux: u32) -> Ref {
        let flags = if op.always_guard() {
            flags | F_GUARD
        } else {
            flags
        };
        self.ir.emit(Ins {
            op,
            t,
            flags,
            a,
            b,
            prev: NOREF,
            aux,
        })
    }
    fn sload(&mut self, slot: u16, t: Ty, snap: u32) -> Ref {
        self.ins(TOp::Sload, t, F_GUARD, slot, NOREF, guard_aux(snap, 1))
    }
    fn poll(&mut self, snap: u32) {
        self.ins(TOp::Poll, Ty::Nil, 0, NOREF, NOREF, guard_aux(snap, 0));
    }
    fn budget(&mut self, n: u16, snap: u32) {
        self.ins(TOp::Budget, Ty::I64, 0, n, NOREF, guard_aux(snap, 0));
    }
    fn kint(&mut self, v: i64) -> Ref {
        self.ir.kint(v)
    }
}

fn model_ints(vals: &[i64]) -> Model {
    let mut m = Model::new();
    for &v in vals {
        m.slots.push(Val::int(v));
    }
    for _ in 0..2 {
        m.tables.push(MTable {
            array: (0..ASIZE).map(|i| Val::int(i as i64 * 10)).collect(),
            metatable: 0,
            flags: 0,
            lsizenode: 0,
            lenhint: 0,
            nodes: Default::default(),
        });
    }
    m.closures.push(vec![Val::int(0); NUPV]);
    m.budget = 100;
    m
}

/// Assembles `ir` (rolled, no writeback) with the default allocator and runs it once in the emulator.
fn exec(
    ir: &TraceIr,
    w: &mut World,
    tweak: impl Fn(&mut AsmEnv),
) -> (u64, Vec<ExitSnap>, Log, Asm) {
    let mut env = base_env(w);
    tweak(&mut env);
    let res = TestRes::linear(ir.snaps.len().max(1));
    let (a, _al, snaps) = compile(ir, &[], None, &env, &res).unwrap_or_else(|e| panic!("{e}"));
    let log = RefCell::new(Log::default());
    let t = run_emu(&a, &snaps, w, &log);
    (t, snaps, log.into_inner(), a)
}

fn snap_of(target: u64) -> Option<u32> {
    (LAND_BASE..LAND_BASE + 16 * 4096)
        .contains(&target)
        .then(|| ((target - LAND_BASE) / 16) as u32)
}

/// A chain of `n` spare `CallInfo` nodes behind the world's `ci`.
fn ci_chain(w: &mut World, nodes: &mut Vec<Vec<u64>>, n: usize) {
    nodes.clear();
    for _ in 0..n {
        nodes.push(vec![0; ci::SIZE.div_ceil(8) + 1]);
    }
    let mut next = 0usize;
    for node in nodes.iter_mut().rev() {
        put(node, ci::NEXT, next);
        next = node.as_ptr() as usize;
    }
    put(&mut w.ci, ci::NEXT, next);
}

#[test]
fn hand_entry_checks_tr6() {
    both(|| {
        // IR: POLL, BUDGET 5 (the budget is 0 in the world, so a trace that gets past the entry exits at snapshot 0)
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        b.poll(s0);
        b.budget(5, s0);
        let base = b.ir.clone();
        let extents: &[u16] = if cfg!(miri) { &[0, 7] } else { &[0, 1, 7, 100] };
        for &extent in extents {
            for depth in 0..=5u8 {
                for chain in 0..=6usize {
                    for margin in [-1i64, 0, 1, 2] {
                        let mut ir = base.clone();
                        ir.extent = extent;
                        ir.depth = depth;
                        let mut w = World::new(&model_ints(&[1]));
                        w.load(&model_ints(&[1]));
                        w.setup_ci(0);
                        let mut nodes = Vec::new();
                        ci_chain(&mut w, &mut nodes, chain);
                        // stack_size - (func + 1 + extent) > 0  <=>  stack_size >= func + 2 + extent
                        let need = (PRE - 1) as i64 + 2 + i64::from(extent);
                        put(&mut w.thread, thr::STACK_SIZE, (need + margin) as usize);
                        w.set_cnt(0);
                        let (t, _, _, _) = exec(&ir, &mut w, |_| {});
                        let room = margin >= 0;
                        let spares = chain >= usize::from(depth);
                        let want = if room && spares { LAND_BASE } else { PCL_H };
                        assert_eq!(
                            t, want,
                            "extent {extent} depth {depth} chain {chain} margin {margin}"
                        );
                        if want == PCL_H {
                            // nothing changed: budget word, frame, thread.ci
                            assert_eq!(w.cnt(), 0);
                        }
                    }
                }
            }
        }
    });
}

#[test]
fn hand_poll_trap_and_interrupt() {
    both(|| {
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(0, &[]);
        b.poll(s0);
        b.budget(5, s1); // the world's budget is 0: reaching it exits at snapshot 1
                         // P-L: the poll tests ONE word (`irq`); `ci.trap` is announced through it (set_hook raises the word), so a trap value
                         // alone no longer exits at the poll: the budget (0) does, at snapshot 1
        for (trap, irq, want) in [
            (0, 0, 1),
            (1, 0, 1),
            (0, 1, 0),
            (1, 1, 0),
            (7, 0, 1),
            (0, 255, 0),
        ] {
            let mut w = World::new(&model_ints(&[1]));
            w.load(&model_ints(&[1]));
            w.setup_ci(trap);
            w.irq[0] = irq;
            w.set_cnt(0);
            let log = RefCell::new(Log::default());
            let res = TestRes::linear(b.ir.snaps.len().max(1));
            let env = base_env(&mut w);
            let (a, _al, snaps) =
                compile(&b.ir, &[], None, &env, &res).unwrap_or_else(|e| panic!("{e}"));
            let t = run_emu(&a, &snaps, &mut w, &log);
            assert_eq!(snap_of(t), Some(want), "trap {trap} irq {irq}");
            // the poll exit is the generic exit tagged EXIT_POLL, counted by nobody
            assert_eq!(
                log.borrow().exit_poll_calls,
                u32::from(irq != 0),
                "trap {trap} irq {irq}"
            );
        }
    });
}

/// F2a (7.8, TR8, TR22): a side trace ends with a poll (the empty map at the head, uncharged), puts the budget register
/// back into the budget word and continues at whatever the dispatch cell of the head holds. Mutants `side_link_no_poll`,
/// `side_link_no_store`.
#[test]
fn hand_side_trace_link_polls_stores_the_budget_and_jumps_through_the_head_cell() {
    use rn_jit::tr::asm::SideEnv;
    both(|| {
        const TARGET: usize = 0x7000_0000;
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(0, &[]); // the link's poll
        b.budget(3, s0);
        let cell = Box::new(Cell::new(TARGET));
        let link_cell = std::ptr::from_ref::<Cell<usize>>(&cell) as u64;
        for (trap, irq, cnt) in [(0, 0, 100), (1, 0, 100), (0, 1, 100), (0, 0, 2)] {
            let mut w = World::new(&model_ints(&[1]));
            w.load(&model_ints(&[1]));
            w.setup_ci(trap);
            w.irq[0] = irq;
            w.set_cnt(cnt);
            let (t, _, _, _) = exec(&b.ir, &mut w, |e| {
                e.side = Some(SideEnv {
                    link_cell,
                    link_snap: s1,
                })
            });
            if cnt < 3 {
                assert_eq!(snap_of(t), Some(s0), "the budget check of the start");
                assert_eq!(w.cnt(), cnt, "uncharged: nothing refunded");
            } else if irq != 0 {
                assert_eq!(
                    snap_of(t),
                    Some(s1),
                    "trap {trap} irq {irq}: the poll of the link"
                );
                assert_eq!(w.cnt(), cnt - 3, "the exit stores the charged budget");
            } else {
                assert_eq!(
                    t, TARGET as u64,
                    "the link continues where the head's cell says"
                );
                assert_eq!(w.cnt(), cnt - 3, "the link stores the budget register");
            }
        }
        // the cell is read at the link, not embedded: change it and the link follows
        cell.set(TARGET + 0x40);
        let mut w = World::new(&model_ints(&[1]));
        w.load(&model_ints(&[1]));
        w.setup_ci(0);
        w.set_cnt(100);
        let (t, _, _, _) = exec(&b.ir, &mut w, |e| {
            e.side = Some(SideEnv {
                link_cell,
                link_snap: s1,
            })
        });
        assert_eq!(t, (TARGET + 0x40) as u64);
    });
}

/// F2a (TR8): with exit cells the inline stub of a frameless exit continues through its cell instead of an embedded
/// address; the cell can be changed after the code exists.
#[test]
fn hand_exit_cells_redirect_inline_stubs() {
    both(|| {
        const SIDE: usize = 0x7100_0000;
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        b.budget(5, s0); // the world's budget is 0: exits at snapshot 0
        let cells: Vec<Cell<usize>> = vec![Cell::new(LAND_BASE as usize)];
        let base = cells.as_ptr() as u64;
        for (want, set) in [(LAND_BASE, None), (SIDE as u64, Some(SIDE))] {
            if let Some(v) = set {
                cells[0].set(v);
            }
            let mut w = World::new(&model_ints(&[1]));
            w.load(&model_ints(&[1]));
            w.setup_ci(0);
            w.set_cnt(0);
            let (t, _, _, _) = exec(&b.ir, &mut w, |e| e.exit_cells = base);
            assert_eq!(t, want);
        }
    });
}

#[test]
fn hand_budget_charge_and_exit_value() {
    both(|| {
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(4, &[]);
        b.budget(3, s0);
        b.budget(3, s1);
        // cnt < 3: exit 0, word unchanged (refund 0); 3 <= cnt < 6: 3 charged, exit 1, word = cnt - 3 + 4
        // (cnt >= 6 passes both and the rolled trace loops: not run)
        for cnt in 0..6 {
            let mut w = World::new(&model_ints(&[1]));
            w.load(&model_ints(&[1]));
            w.setup_ci(0);
            w.set_cnt(cnt);
            let (t, _, _, _) = exec(&b.ir, &mut w, |_| {});
            let (snap, word) = if cnt < 3 { (0, cnt) } else { (1, cnt - 3 + 4) };
            assert_eq!(snap_of(t), Some(snap), "cnt {cnt}");
            assert_eq!(w.cnt(), word, "cnt {cnt}");
        }
    });
}

#[test]
fn hand_entry_failure_bookkeeping_follows_the_free_peel_rule() {
    both(|| {
        use rn_jit::tr::cache::{HeadData, HeadState};
        use std::cell::Cell;
        // IR: BUDGET 1 (snapshot 0), SLOAD int slot 0 with an empty-map entry snapshot (an entry check)
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(1, &[]);
        b.budget(1, s0);
        b.sload(0, Ty::Int, s1);
        b.budget(1000, s0); // never charged: ends the trace through snapshot 0 once the entry succeeded
        let head = Box::new(HeadData {
            cell: Cell::new(0),
            ctr: Cell::new(0),
            pending: Cell::new(0),
            entry_fail: Cell::new(0),
            entry_ok: Cell::new(0),
            entry: Cell::new(0),
            pc: 0,
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
            dom_done: Cell::new(false),
            ncallee: Cell::new(false),
            nohoist: Cell::new(false),
            rearm_budget: Cell::new(0),
            rearm_wait: Cell::new(0),
            rearm_last: Cell::new(0),
            second: Cell::new(false),
            inner_n: Cell::new(0),
            ident_kept: Cell::new(false),
            pre: Cell::new(0),
            rec_inner: Cell::new(0),
            rec: Cell::new(0),
            lazy: Cell::new(false),
            deep: Cell::new(false),
        });
        let hp = std::ptr::addr_of!(*head) as u64;
        let mut rng = Rng::new(99);
        let (mut pending, mut fail, mut ok) = (0u32, 0u32, 0u32);
        let mut expect_calls = 0u32;
        for step in 0..(if cfg!(miri) { 60 } else { 400 }) {
            // outcome: 0 success, 1 typecheck failure, 2 stack-room failure (TR6a)
            let outcome = match rng.below(10) {
                0..=2 => 0,
                3..=8 => 1,
                _ => 2,
            };
            let mut m = model_ints(&[5]);
            if outcome == 1 {
                m.slots[0] = Val::num(1.5);
            }
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            w.set_cnt(10);
            if outcome == 2 {
                put(&mut w.thread, thr::STACK_SIZE, 1usize);
            }
            let mut ir = b.ir.clone();
            ir.extent = 3;
            let (t, _, log, _) = exec(&ir, &mut w, |e| {
                e.head = hp;
                e.head_pc = 77;
            });
            match outcome {
                0 => {
                    pending = 0;
                    ok += 1;
                    assert_eq!(snap_of(t), Some(0), "step {step}");
                }
                _ => {
                    fail += pending;
                    pending = 1;
                    if outcome == 1 {
                        assert_eq!(snap_of(t), Some(1), "step {step}");
                    } else {
                        assert_eq!(t, PCL_H, "step {step}");
                    }
                    if fail >= 64 && u64::from(fail) * 4 > u64::from(ok) {
                        expect_calls += 1;
                    }
                }
            }
            assert_eq!(
                (
                    head.pending.get(),
                    head.entry_fail.get(),
                    head.entry_ok.get()
                ),
                (pending, fail, ok),
                "step {step} outcome {outcome}"
            );
            // the helper runs on every failure at or above the threshold, as `note_entry_fail` reports it
            assert_eq!(
                log.entry_fail_calls,
                u32::from(outcome != 0 && fail >= 64 && u64::from(fail) * 4 > u64::from(ok))
            );
        }
        assert!(
            cfg!(miri) || expect_calls > 0,
            "the threshold was never reached"
        );
    });
}

#[test]
fn hand_hot_exit_diverts_to_the_generic_stub() {
    both(|| {
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(2, &[]);
        b.budget(1, s0);
        b.sload(0, Ty::Int, s1);
        b.sload(1, Ty::Int, s1); // keeps a value live in a register at the exit
        let ka = b.ir.kint(3);
        let sum = b.ins(TOp::Add, Ty::Int, 0, REF_BIAS + 2, ka, 0);
        let lim = b.ir.kint(1_000_000);
        let s2 = {
            let e = [SnapEntry {
                slot: 4,
                r: sum,
                flags: 0,
            }];
            b.snap(1, &e)
        };
        b.ins(TOp::Lt, Ty::Int, 0, sum, lim, guard_aux(s2, 0)); // fails when the sum is >= 1_000_000
        let hot = 3u32;
        let mut m = model_ints(&[5, 2_000_000]);
        m.budget = 50;
        let mut w = World::new(&m);
        let (mut carried, mut inline_runs, mut generic_runs) = (0u32, 0, 0);
        for run in 0..6u32 {
            w.load(&m);
            w.setup_ci(0);
            w.counters[s2 as usize] = carried;
            let (t, _, log, _) = exec(&b.ir, &mut w, |e| {
                e.hot_exit = hot;
            });
            assert_eq!(snap_of(t), Some(s2), "run {run}");
            // the restored state is right on either path
            let d = w.dump();
            assert_eq!(d.frame[4], (u64::from(LUA_VNUMINT), 2_000_003), "run {run}");
            assert_eq!(d.cnt, 50, "run {run}: charge 1, refund 1");
            if log.exit_calls == 0 {
                inline_runs += 1;
                assert_ne!(carried, hot - 1, "run {run}: the divert fires at hot - 1");
                assert_eq!(
                    w.counters[s2 as usize],
                    carried + 1,
                    "run {run}: counted inline"
                );
            } else {
                generic_runs += 1;
                assert_eq!(carried, hot - 1, "run {run}: diverted exactly at hot - 1");
                assert_eq!(log.exit_calls, 1);
                // the handler (not modelled by `Hlp`) counts; the inline code must not have
                assert_eq!(
                    w.counters[s2 as usize], carried,
                    "run {run}: not counted twice"
                );
                w.counters[s2 as usize] += 1; // what `exit_core` does
            }
            carried = w.counters[s2 as usize];
        }
        assert_eq!(
            (inline_runs, generic_runs),
            (5, 1),
            "the divert fires once (counter == hot - 1); the inline path counts on afterwards"
        );
    });
}

/// A fresh head record for the entry-bookkeeping tests.
fn new_head() -> Box<rn_jit::tr::cache::HeadData> {
    use rn_jit::tr::cache::{HeadData, HeadState};
    Box::new(HeadData {
        cell: Cell::new(0),
        ctr: Cell::new(0),
        pending: Cell::new(0),
        entry_fail: Cell::new(0),
        entry_ok: Cell::new(0),
        entry: Cell::new(0),
        pc: 0,
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
        dom_done: Cell::new(false),
        ncallee: Cell::new(false),
        nohoist: Cell::new(false),
        rearm_budget: Cell::new(0),
        rearm_wait: Cell::new(0),
        rearm_last: Cell::new(0),
        second: Cell::new(false),
        inner_n: Cell::new(0),
        ident_kept: Cell::new(false),
        pre: Cell::new(0),
        rec_inner: Cell::new(0),
        rec: Cell::new(0),
        lazy: Cell::new(false),
        deep: Cell::new(false),
    })
}

/// [`exec`] with a writeback list (a rolled trace stores it at the end of every iteration).
fn exec_wb(
    ir: &TraceIr,
    wb: &[SnapEntry],
    w: &mut World,
    tweak: impl Fn(&mut AsmEnv),
) -> (u64, Log) {
    let mut env = base_env(w);
    tweak(&mut env);
    let res = TestRes::linear(ir.snaps.len().max(1));
    let (a, _al, snaps) = compile(ir, wb, None, &env, &res).unwrap_or_else(|e| panic!("{e}"));
    let log = RefCell::new(Log::default());
    let t = run_emu(&a, &snaps, w, &log);
    (t, log.into_inner())
}

#[test]
fn hand_rolled_early_typecheck_in_iteration_two_is_not_an_entry_failure() {
    both(|| {
        // BUDGET; x = SLOAD int slot 0 (empty map: an entry-check *candidate*); y = (float) x; writeback slot 0 := y.
        // Iteration 1 reads an int and writes a float to the slot: the SLOAD of iteration 2 fails. That is a side
        // exit of the running trace, not an entry failure (3.3): the head counters must not move.
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(1, &[]);
        b.budget(1, s0);
        let x = b.sload(0, Ty::Int, s1);
        let y = b.ins(TOp::ConvNumInt, Ty::Num, 0, x, NOREF, 0);
        let wb = [SnapEntry {
            slot: 0,
            r: y,
            flags: 0,
        }];
        for generic_only in [false, true] {
            // iteration 1 passes, iteration 2 fails
            let head = new_head();
            head.pending.set(1);
            let hp = std::ptr::addr_of!(*head) as u64;
            let m = model_ints(&[5]);
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            w.set_cnt(10);
            let (t, log) = exec_wb(&b.ir, &wb, &mut w, |e| {
                e.head = hp;
                e.head_pc = 7;
                e.generic_only = generic_only;
            });
            assert_eq!(snap_of(t), Some(s1), "generic_only {generic_only}");
            assert_eq!(
                (
                    head.pending.get(),
                    head.entry_fail.get(),
                    head.entry_ok.get()
                ),
                (0, 0, 1),
                "iteration 2 is not an entry failure (generic_only {generic_only})"
            );
            assert_eq!(
                (log.entry_fail_calls, log.exit_entry_calls),
                (0, 0),
                "generic_only {generic_only}"
            );
            // iteration 1 fails: that *is* an entry failure (counted: the previous attempt failed as well)
            let head = new_head();
            head.pending.set(1);
            let hp = std::ptr::addr_of!(*head) as u64;
            let mut m = model_ints(&[5]);
            m.slots[0] = Val::num(1.5);
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            w.set_cnt(10);
            let (t, log) = exec_wb(&b.ir, &wb, &mut w, |e| {
                e.head = hp;
                e.head_pc = 7;
                e.generic_only = generic_only;
            });
            assert_eq!(snap_of(t), Some(s1));
            assert_eq!(
                (
                    head.pending.get(),
                    head.entry_fail.get(),
                    head.entry_ok.get()
                ),
                (1, 1, 0),
                "iteration 1 is an entry failure (generic_only {generic_only})"
            );
            assert_eq!(
                log.exit_entry_calls,
                u32::from(generic_only),
                "the generic twin of an entry check carries the entry flag"
            );
        }
    });
}

#[test]
fn hand_peeled_entry_checks_are_exactly_the_pre_roll_guards() {
    both(|| {
        // SLOAD slot 0 (pre-roll, empty map: an entry check); LOOP; SLOAD slot 1 after LOOP (empty map but inside
        // the loop: an ordinary guard). Failures of the second must not touch the head counters.
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(1, &[]);
        let s2 = b.snap(2, &[]);
        b.sload(0, Ty::Int, s1);
        let lp = b.ins(TOp::Loop, Ty::Nil, 0, NOREF, NOREF, 0);
        b.ir.loop_at = Some(lp);
        b.sload(1, Ty::Int, s2);
        b.budget(1, s0);
        for (slot0_float, slot1_float, want_snap, want) in [
            (true, false, s1, (1u32, 1u32, 0u32)),
            (false, true, s2, (0, 0, 1)),
        ] {
            let head = new_head();
            head.pending.set(1);
            let hp = std::ptr::addr_of!(*head) as u64;
            let mut m = model_ints(&[5, 6]);
            if slot0_float {
                m.slots[0] = Val::num(1.5);
            }
            if slot1_float {
                m.slots[1] = Val::num(1.5);
            }
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            w.set_cnt(10);
            let (t, _) = exec_wb(&b.ir, &[], &mut w, |e| {
                e.head = hp;
                e.head_pc = 7;
            });
            assert_eq!(snap_of(t), Some(want_snap));
            assert_eq!(
                (
                    head.pending.get(),
                    head.entry_fail.get(),
                    head.entry_ok.get()
                ),
                want,
                "slot0 float {slot0_float}"
            );
        }
    });
}

#[test]
fn hand_constant_array_indexes_beyond_a_displacement_are_unsupported() {
    both(|| {
        for store in [false, true] {
            for (idx, ok) in [
                (3i64, true),
                ((1 << 28) - 1, true),
                (1 << 28, false),
                (1 << 40, false),
                (-1, false),
            ] {
                let mut b = B::new();
                let s0 = b.snap(0, &[]);
                let s1 = b.snap(0, &[]);
                let t = b.sload(0, Ty::Tab, s1);
                let arr = b.ins(TOp::Fload, Ty::P64, 0, t, Field::Array as Ref, 0);
                let k = b.kint(idx);
                let aref = b.ins(TOp::Aref, Ty::P64, 0, arr, k, 0);
                if store {
                    let v = b.sload(1, Ty::Int, s1);
                    b.ins(TOp::Astore, Ty::Nil, 0, aref, v, 0);
                } else {
                    b.ins(TOp::Aload, Ty::Int, F_GUARD, aref, NOREF, guard_aux(s1, 1));
                }
                b.budget(1000, s0);
                let mut m = model_ints(&[]);
                m.slots.push(Val {
                    t: Ty::Tab,
                    bits: 1,
                });
                m.slots.push(Val::int(1));
                let mut w = World::new(&m);
                let env = base_env(&mut w);
                let res = TestRes::linear(2);
                let r = compile(&b.ir, &[], None, &env, &res);
                match (ok, r) {
                    (true, Ok(_)) => {}
                    (false, Err(e)) => {
                        assert!(e.contains("Unsupported"), "store {store} index {idx}: {e}")
                    }
                    (ok, r) => panic!(
                        "store {store} index {idx}: want ok {ok}, got {:?}",
                        r.map(|_| ())
                    ),
                }
            }
        }
    });
}

#[test]
fn hand_entry_ok_saturates_instead_of_wrapping() {
    both(|| {
        use rn_jit::tr::cache::ENTRY_OK_CAP;
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(1, &[]);
        b.budget(1, s0);
        b.sload(0, Ty::Int, s1);
        b.budget(1000, s0);
        for (start, want) in [
            (0u32, 1u32),
            (ENTRY_OK_CAP - 1, ENTRY_OK_CAP),
            (ENTRY_OK_CAP, ENTRY_OK_CAP),
            (u32::MAX, u32::MAX),
        ] {
            let head = new_head();
            head.entry_ok.set(start);
            head.entry_fail.set(64);
            let hp = std::ptr::addr_of!(*head) as u64;
            let m = model_ints(&[5]);
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            w.set_cnt(10);
            let mut ir = b.ir.clone();
            ir.extent = 3;
            let (t, _, log, _) = exec(&ir, &mut w, |e| {
                e.head = hp;
                e.head_pc = 7;
            });
            assert_eq!(snap_of(t), Some(0));
            assert_eq!(head.entry_ok.get(), want, "start {start}");
            assert_eq!(log.entry_fail_calls, 0);
        }
    });
}

#[test]
fn hand_tbar_calls_the_barrier_exactly_for_black_tables_and_white_values() {
    both(|| {
        use rn_core::gc::{BLACKBIT, WHITEBITS};
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(0, &[]);
        let t = b.sload(0, Ty::Tab, s1);
        let v = b.sload(1, Ty::Tab, s1);
        let arr = b.ins(TOp::Fload, Ty::P64, 0, t, Field::Array as Ref, 0);
        let k2 = b.kint(2);
        let aref = b.ins(TOp::Aref, Ty::P64, 0, arr, k2, 0);
        b.ins(TOp::Astore, Ty::Nil, 0, aref, v, 0);
        b.ins(TOp::Tbar, Ty::Nil, 0, t, v, 0);
        b.budget(1000, s0);
        for (tmark, vmark, want) in [
            (0u8, 0u8, 0usize),
            (BLACKBIT, 0, 0),
            (0, WHITEBITS, 0),
            (BLACKBIT, WHITEBITS, 1),
            (BLACKBIT | 3, 1 << 3, 1),
            (BLACKBIT, 1 << 4, 1),
            (BLACKBIT, 1 << 5, 0),
        ] {
            let mut m = model_ints(&[]);
            m.slots.push(Val {
                t: Ty::Tab,
                bits: 1,
            });
            m.slots.push(Val {
                t: Ty::Tab,
                bits: 2,
            });
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            put(&mut w.tabs[0].hdr, table::MARKED, tmark);
            put(&mut w.tabs[1].hdr, table::MARKED, vmark);
            w.set_cnt(0);
            let (t_, _, log, _) = exec(&b.ir, &mut w, |_| {});
            assert_eq!(snap_of(t_), Some(0));
            assert_eq!(log.barriers.len(), want, "marks {tmark:#x} {vmark:#x}");
            if want == 1 {
                let (ta, va) = (w.tab_addr(1), w.tab_addr(2));
                assert_eq!(log.barriers[0], (ta, va, u32::from(ctb(LUA_VTABLE))));
            }
            // the store itself happened regardless: element 2 of table 1 is table 2, with the table tag
            let arr = get::<usize>(&w.tabs[0].hdr, table::ARRAY);
            // SAFETY: element 2 of a table loaded by `load`.
            let (tag, pay) = unsafe {
                (
                    (arr as *const u8).add(table::ARRAY_TAG_BASE + 2).read(),
                    (arr as *const u8).sub(24).cast::<u64>().read_unaligned(),
                )
            };
            assert_eq!((tag, pay), (ctb(LUA_VTABLE), w.tab_addr(2)));
        }
    });
}

/// BUNDLE-V V2: `TBAR uref, v` with `aux` 1 (the barrier of an upvalue) calls the helper (with `b` = 1, `args[0]` = the `UpVal`) exactly for a black upvalue and a white value;
/// the `USTORE` happens regardless. Mutant `upbar_asm_skip` (no call).
#[test]
fn hand_upvalue_tbar_calls_the_barrier_exactly_for_black_upvalues_and_white_values() {
    both(|| {
        use rn_core::gc::{BLACKBIT, WHITEBITS};
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(0, &[]);
        let cl = b.ins(TOp::Sload, Ty::Lcf, 0, 0xFFFF, NOREF, 0);
        let v = b.sload(0, Ty::Tab, s1);
        let k0 = b.kint(0);
        let uref = b.ins(TOp::Uref, Ty::P64, 0, cl, k0, 0);
        b.ins(TOp::Ustore, Ty::I64, 0, uref, v, 0);
        b.ins(TOp::Tbar, Ty::I64, 0, uref, v, 1);
        b.budget(1000, s0);
        for (umark, vmark, want) in [
            (0u8, 0u8, 0usize),
            (BLACKBIT, 0, 0),
            (0, WHITEBITS, 0),
            (BLACKBIT, WHITEBITS, 1),
            (BLACKBIT | 3, 1 << 3, 1),
            (BLACKBIT, 1 << 4, 1),
            (BLACKBIT, 1 << 5, 0),
        ] {
            let mut m = model_ints(&[]);
            m.slots.push(Val {
                t: Ty::Tab,
                bits: 1,
            });
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            put(&mut w.clo.upvals[0], table::MARKED, umark);
            put(&mut w.tabs[0].hdr, table::MARKED, vmark);
            w.set_cnt(0);
            let uv = w.clo.upvals[0].as_mut_ptr() as u64;
            let (t_, _, log, _) = exec(&b.ir, &mut w, |_| {});
            assert_eq!(snap_of(t_), Some(0));
            assert_eq!(log.barriers.len(), want, "marks {umark:#x} {vmark:#x}");
            if want == 1 {
                let va = w.tab_addr(1);
                assert_eq!(log.barriers[0], (uv, va, u32::from(ctb(LUA_VTABLE))));
                assert_eq!(log.barrier_bs, vec![1]);
            }
        }
    });
}

/// 5.4 `alimit_touch` of `table.rs` on a model (the reference for `ATOUCH`).
#[cfg(rustynail_lua54)]
fn ref_alimit_touch(ras: bool, lim: u32, u: u64) -> u32 {
    let l = u64::from(lim);
    if ras && u >= l && (u & !l.wrapping_sub(1)) < l {
        (u + 1) as u32
    } else {
        lim
    }
}

/// F7a (TR30): `ATOUCH` (constant and register index, with and without `BITRAS`, every limit and index of an 8-element
/// array) leaves exactly the array limit `alimit_touch` leaves, and touches nothing else; the register form must not
/// disturb the registers around it (the sequence saves its temporaries).
#[cfg(rustynail_lua54)]
#[test]
fn hand_atouch_follows_alimit_touch() {
    both(|| {
        for const_idx in [false, true] {
            for ras in [false, true] {
                for lim in 0..=ASIZE as u32 {
                    for u in 0..ASIZE as u64 {
                        let mut b = B::new();
                        let s0 = b.snap(0, &[]);
                        let t = b.sload(0, Ty::Tab, s0);
                        let x = b.sload(2, Ty::Int, s0);
                        let idx = if const_idx {
                            b.kint(u as i64)
                        } else {
                            b.sload(1, Ty::Int, s0)
                        };
                        b.ins(TOp::Atouch, Ty::Nil, 0, t, idx, 0);
                        // `x` and `idx` stay live across the touch (their registers must survive it)
                        let y = b.ins(TOp::Add, Ty::Int, 0, x, idx, 0);
                        b.ins(TOp::Add, Ty::Int, 0, y, x, 0);
                        b.budget(1000, s0);
                        let mut m = model_ints(&[]);
                        m.slots.push(Val {
                            t: Ty::Tab,
                            bits: 1,
                        });
                        m.slots.push(Val::int(u as i64));
                        m.slots.push(Val::int(77));
                        m.tables[0].lenhint = lim;
                        if ras {
                            m.tables[0].flags |= rn_jit::tr::interp::MODEL_BITRAS;
                        }
                        m.tables[1].lenhint = 3;
                        m.tables[1].flags |= rn_jit::tr::interp::MODEL_BITRAS;
                        let mut w = World::new(&m);
                        w.load(&m);
                        w.setup_ci(0);
                        w.set_cnt(0);
                        let (t_, _, _, _) = exec(&b.ir, &mut w, |_| {});
                        assert_eq!(snap_of(t_), Some(0));
                        let d = w.dump();
                        assert_eq!(
                            d.hints,
                            vec![ref_alimit_touch(ras, lim, u), 3],
                            "const {const_idx} ras {ras} lim {lim} u {u}"
                        );
                        // the model agrees
                        let mut mm = m.clone();
                        rn_jit::tr::interp::run(&b.ir, &mut mm).unwrap();
                        assert_eq!(mm.tables[0].lenhint, ref_alimit_touch(ras, lim, u));
                    }
                }
            }
        }
    });
}

#[test]
fn hand_uvrel_range_guard_exits_for_upvalues_inside_the_frame() {
    both(|| {
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(0, &[]);
        let cl = b.ins(TOp::Sload, Ty::Lcf, 0, 0xFFFF, NOREF, 0);
        let k0 = b.kint(0);
        let uref = b.ins(TOp::Uref, Ty::P64, 0, cl, k0, 0);
        let rel = b.ins(TOp::Uvrel, Ty::I64, 0, uref, NOREF, 0);
        let lim = b.kint(16 * 4);
        b.ins(TOp::Uge, Ty::I64, 0, rel, lim, guard_aux(s1, 0));
        b.budget(1000, s0);
        // slot offsets relative to the root base, in bytes: inside [0, 64) the guard must exit
        for (off, inside) in [
            (0i64, true),
            (16, true),
            (48, true),
            (63, true),
            (64, false),
            (-16, false),
            (1 << 20, false),
            (-(1 << 20), false),
        ] {
            let m = model_ints(&[1]);
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            let rbx = w.rbx() as i64;
            // the upvalue's value pointer: `uv.v = rbx + off` (never dereferenced by this IR)
            put(&mut w.clo.upvals[0], upval::V, (rbx + off) as usize);
            w.set_cnt(0);
            let (t, _, _, _) = exec(&b.ir, &mut w, |_| {});
            assert_eq!(snap_of(t), Some(u32::from(inside)), "offset {off}");
        }
    });
}

#[test]
fn hand_eqref_proto_and_builtin_forms() {
    both(|| {
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(0, &[]);
        let s2 = b.snap(0, &[]);
        let cl = b.ins(TOp::Sload, Ty::Lcf, 0, 0xFFFF, NOREF, 0);
        let f = b.sload(0, Ty::Lfn, s2);
        let want_proto: u64 = 0x1234_5678_9ABC_DEF0;
        let kp = b.ir.konst(KVal::Ptr(want_proto, PtrKind::Proto));
        let want_fn: u64 = 0x0000_7FFF_1234_5670;
        let kf = b.ir.konst(KVal::Ptr(want_fn, PtrKind::Builtin));
        b.ins(TOp::Eqref, Ty::Lcf, 0, cl, kp, guard_aux(s1, 0));
        b.ins(TOp::Eqref, Ty::Lfn, 0, f, kf, guard_aux(s2, 0));
        b.budget(1000, s0);
        let cases: [(u64, u64, u32); 8] = [
            (want_proto, want_fn, 0),
            (want_proto ^ 1, want_fn, 1),
            (want_proto ^ (1 << 32), want_fn, 1),
            (want_proto ^ (1 << 63), want_fn, 1),
            (0, want_fn, 1),
            (want_proto, want_fn ^ 1, 2),
            (want_proto, want_fn ^ (1 << 40), 2),
            (want_proto, 0, 2),
        ];
        for (p, fnaddr, want) in cases {
            let mut m = model_ints(&[]);
            m.slots.push(Val {
                t: Ty::Lfn,
                bits: fnaddr,
            });
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            put(&mut w.clo.mem, lclosure::PROTO, p as usize);
            w.set_cnt(0);
            let (t, _, _, _) = exec(&b.ir, &mut w, |_| {});
            assert_eq!(snap_of(t), Some(want), "proto {p:#x} fn {fnaddr:#x}");
        }
    });
}

/// J1-1: `HREFK` + `HLOAD` with two snapshots of identical content (since `typesides` the type guard has its own): the pair is fused
/// (one 16-bit tag compare) and a failed compare still leaves through the exit of the guard that failed: a key miss through the
/// `HREFK`'s snapshot, a value of another type through the `HLOAD`'s (told apart by the exit counters). Also the 64-bit key compare of
/// `HMISS`. Detector of the mutants `hkfuse_split_never` (the pair is not fused), `hkfuse_cold_swap`, `hmiss_key64_hi`.
#[test]
fn hand_fused_hrefk_hload_keeps_both_exits() {
    both(|| {
        let mut b = B::new();
        let s0 = b.ir.snapshot(Snap::default(), &[]);
        let sa = b.ir.snapshot(Snap::default(), &[]);
        let sb = b.ir.snapshot(Snap::default(), &[]);
        let sm = b.ir.snapshot(Snap::default(), &[]);
        let t = b.sload(0, Ty::Tab, s0);
        let node = b.ins(TOp::Fload, Ty::P64, 0, t, Field::Node as Ref, 0);
        let want: u64 = 0x1_0000_1000;
        let kc = b.ir.konst(KVal::Ptr(want, PtrKind::Raw));
        let hr = b.ins(TOp::Hrefk, Ty::P64, F_GUARD, node, kc, guard_aux(sa, 0));
        b.ins(TOp::Hload, Ty::Int, F_GUARD, hr, NOREF, guard_aux(sb, 1));
        // node 1 does not hold `want` and links on by 1
        let bits = hmiss_bits(1, 1).expect("small index");
        b.ins(TOp::Hmiss, Ty::I64, F_GUARD, node, kc, guard_aux(sm, bits));
        b.budget(1000, s0);
        let nd = |key: u64, v: Val, next: i32| interp::MNode {
            key,
            val: v,
            next,
            int: false,
        };
        // (key of node 0, value of node 0, key of node 1, expected exit snapshot)
        let cases: [(u64, Val, u64, u32); 8] = [
            (want, Val::int(5), 0x2000, s0),
            (want ^ 1, Val::int(5), 0x2000, sa),
            (want ^ (1 << 32), Val::int(5), 0x2000, sa),
            (want ^ (1 << 40), Val::int(5), 0x2000, sa),
            (want, Val::num(1.5), 0x2000, sb),
            (want, Val::int(5), want, sm),
            (want, Val::int(5), want ^ (1 << 40), s0),
            (want, Val::int(5), want ^ 1, s0),
        ];
        for (k0, v0, k1, want_exit) in cases {
            let mut m = model_ints(&[]);
            m.slots.push(Val {
                t: Ty::Tab,
                bits: 1,
            });
            m.tables[0].lsizenode = 2;
            m.tables[0].nodes.insert(0, nd(k0, v0, 0));
            m.tables[0].nodes.insert(1, nd(k1, Val::int(0), 1));
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            w.set_cnt(0);
            let mut fused = 0;
            let (_, _, _, a) = exec(&b.ir, &mut w, |e| e.hk_fuse = true);
            fused += a.hk_fused;
            let counted: Vec<u32> = [s0, sa, sb, sm]
                .iter()
                .map(|&x| w.counters[x as usize])
                .collect();
            let exits: Vec<u32> = [s0, sa, sb, sm]
                .iter()
                .map(|&x| u32::from(x == want_exit))
                .collect();
            assert_eq!(counted, exits, "node0 key {k0:#x} val {v0:?} node1 key {k1:#x}: expected the exit of snapshot {want_exit}");
            if b4_knobs() {
                assert_eq!(
                    fused, 1,
                    "the HREFK+HLOAD pair with two snapshots was not fused"
                );
            }
        }
    });
}

/// J1-2: an exit that materialises a sunk table is restored by its inline stub when `sink_stub` is on: the stub counts the exit, stores
/// the snapshot's slots and the budget, saves the registers the table's fields read into the exit area and calls the materialising helper
/// (test double), then jumps to the landing label; without the knob the generic stub goes through the exit handler. A failing helper makes
/// the stub return `JitExit::Error`. Detector of the mutants `sinkstub_never`, `sinkstub_save_skip`, `sinkstub_err_ignored`.
#[test]
fn hand_sunk_exit_stub_calls_the_materialising_helper() {
    both(|| {
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let v = b.sload(0, Ty::Int, s0);
        let w2 = b.sload(1, Ty::Int, s0);
        let key = b.ir.konst(KVal::Ptr(0x2000, PtrKind::Raw));
        let key2 = b.ir.konst(KVal::Ptr(0x2018, PtrKind::Raw));
        let hdr = b.kint(2); // NEWTABLE B = 2 (two hash slots), C = 0
        let entries = [
            SnapEntry {
                slot: 3,
                r: hdr,
                flags: SNAP_SUNK,
            },
            SnapEntry {
                slot: SNAP_PSEUDO,
                r: key,
                flags: SNAP_SFKEY,
            },
            SnapEntry {
                slot: SNAP_PSEUDO,
                r: v,
                flags: SNAP_SFVAL,
            },
            SnapEntry {
                slot: SNAP_PSEUDO,
                r: key2,
                flags: SNAP_SFKEY,
            },
            SnapEntry {
                slot: SNAP_PSEUDO,
                r: w2,
                flags: SNAP_SFVAL,
            },
            SnapEntry {
                slot: 4,
                r: v,
                flags: 0,
            },
        ];
        let s1 = b.snap(0, &entries);
        b.budget(1000, s1);
        let mk = || {
            let mut m = model_ints(&[0x1111_2222_3333, 0x4444_5555_6666]);
            m.budget = 100;
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            w.set_cnt(0);
            w
        };
        // the stub
        for fail in [false, true] {
            let mut w = mk();
            let mut env = base_env(&mut w);
            env.sink_stub = true;
            let res = TestRes::linear(b.ir.snaps.len());
            let (a, _al, snaps) =
                compile(&b.ir, &[], None, &env, &res).unwrap_or_else(|e| panic!("{e}"));
            let log = RefCell::new(Log {
                mat_fail: fail,
                ..Log::default()
            });
            let envr = w.env();
            let mut hl = Hlp {
                snaps: &snaps,
                stack_size: PRE + NSLOT,
                stack0: w.frame.as_mut_ptr().cast(),
                root: w.ci.as_mut_ptr().cast(),
                code: w.code.as_ptr(),
                log: &log,
            };
            let mut emu = Emu::new(&a.prog, a.entry);
            // SAFETY: the buffers of `w` cover every offset the trace uses.
            let u = unsafe { emu.run(&a.prog, &[], &envr, 3_000_000, &mut hl) };
            let l = log.into_inner();
            assert_eq!(l.exit_calls, 0, "the handler was called");
            assert_eq!(l.mat_calls.len(), 1, "the helper was not called once");
            let (snap, trace, words) = &l.mat_calls[0];
            assert_eq!((*snap, *trace), (s1, 0));
            assert!(
                words.contains(&0x1111_2222_3333),
                "the register of the first field value was not saved: {words:x?}"
            );
            assert!(
                words.contains(&0x4444_5555_6666),
                "the register of the second field value was not saved: {words:x?}"
            );
            if fail {
                assert!(
                    matches!(u, Until::Stop(Stop::Ret(3))),
                    "a failing helper must return JitExit::Error: {u:?}"
                );
            } else {
                assert!(matches!(u, Until::Stop(Stop::Unsupported)), "{u:?}");
                assert_eq!(snap_of(emu.jmp_target().expect("a JmpR target")), Some(s1));
                // the stub stored the plain slot and counted the exit
                assert_eq!(w.counters[s1 as usize], 1);
                let (tag, bits) = (w.frame[(PRE + 4) * 2], w.frame[(PRE + 4) * 2 + 1]);
                assert_eq!(
                    (tag, bits),
                    (tp(Val::int(0x1111_2222_3333)).0, 0x1111_2222_3333)
                );
            }
        }
        // the knob off: the generic stub and the exit handler
        let mut w = mk();
        let (_, _, log, _) = exec(&b.ir, &mut w, |_| {});
        assert_eq!((log.exit_calls, log.mat_calls.len()), (1, 0));
    });
}

#[test]
fn hand_entry_guards_are_never_moved() {
    both(|| {
        // The IR order is the code order: an EQREF that uses a snapshot (here the "entry snapshot" shape: empty map,
        // charged) stays behind the BUDGET that charges it, in both layouts. Observable: with an exhausted budget the
        // trace leaves through the BUDGET snapshot (uncharged) before it can reach the guard.
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let entry = b.snap(5, &[]); // charged: refund = n
        b.budget(5, s0);
        let cl = b.ins(TOp::Sload, Ty::Lcf, 0, 0xFFFF, NOREF, 0);
        let kp = b.ir.konst(KVal::Ptr(0x1000, PtrKind::Proto));
        b.ins(TOp::Eqref, Ty::Lcf, 0, cl, kp, guard_aux(entry, 0));
        let m = model_ints(&[]);
        let mut w = World::new(&m);
        w.load(&m);
        w.setup_ci(0);
        put(&mut w.clo.mem, lclosure::PROTO, 0x2000usize);
        // budget 4 < 5: exits at S0 (refund 0), the guard (which would fail) is not reached
        w.set_cnt(4);
        let (t, _, _, a) = exec(&b.ir, &mut w, |_| {});
        assert_eq!((snap_of(t), w.cnt()), (Some(0), 4));
        // budget 9: charged 5, then the guard fails and refunds 5
        w.load(&m);
        w.setup_ci(0);
        put(&mut w.clo.mem, lclosure::PROTO, 0x2000usize);
        w.set_cnt(9);
        let (t, _, _, _) = exec(&b.ir, &mut w, |_| {});
        assert_eq!((snap_of(t), w.cnt()), (Some(1), 9));
        // and structurally: the compare of the guard comes after the budget's store in the micro-ops
        use rn_jit::ir::Op;
        let ops = &a.prog.ops;
        // the budget's charge: the store of the word, or (register form) the `Sub` of the budget register
        let st = match vb_reg() {
            Some(r) => ops
                .iter()
                .position(|o| matches!(o, Op::Sub(d, rn_jit::ir::Src::Imm(5)) | Op::SubJl { d, imm: 5, .. } if *d == r)),
            None => ops.iter().position(|o| {
                matches!(o, Op::St { w: rn_jit::ir::W::B4, m, .. }
                    if m.off == 0 && m.base == rn_jit::ir::Base::Cnt)
            }),
        }
        .expect("budget charge");
        let cmp = ops
            .iter()
            .position(|o| matches!(o, Op::BrM { .. }))
            .expect("proto compare");
        assert!(st < cmp, "the guard stays behind the budget charge");
    });
}

// ---- 4. end to end: recorded traces of real Lua loops, installed and entered through the dispatch cells ------------

#[cfg(not(miri))]
mod e2e {
    use super::*;
    use rn_core::jit_hooks::{self, Abort, RecStep, RecVTable};
    use rn_core::state::{Ci, LuaState, State, LUA_MULTRET};
    use rn_core::status::{LuaResult, Status};
    use rn_core::value::Value;
    use rn_jit::tr::cache::{self as c, HeadData, ProtoRef, TrConfig, TrId, TrMode, TraceInstall};
    use rn_jit::tr::record::{self, AbortReason, RecCfg, RecOutcome, Recorded};
    use rn_jit::{Config, Mode};
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::Once;

    /// Knobs of one traced state.
    #[derive(Clone, Debug)]
    pub struct Opts {
        pub exitstorm: Option<u32>,
        pub generic_only: bool,
        pub randra: Option<u64>,
        pub hot_loop: i32,
        pub hot_exit: u32,
        pub count_budget: Option<i32>,
        /// Collect garbage between the calls of the driver (barrier stress).
        pub gc_between: bool,
        /// `+tracemove`: the stack moves right before every trace entry.
        pub tracemove: bool,
    }

    impl Default for Opts {
        fn default() -> Opts {
            Opts {
                exitstorm: None,
                generic_only: false,
                randra: None,
                hot_loop: 4,
                hot_exit: 16,
                count_budget: None,
                gc_between: false,
                tracemove: false,
            }
        }
    }

    #[derive(Default)]
    struct Ctx {
        protos: HashMap<usize, ProtoRef>,
        recs: HashMap<u64, (TrId, ProtoRef, usize)>,
        opts: Opts,
        installed: u32,
        aborted: Vec<String>,
        failed: Vec<String>,
        dumps: Vec<String>,
    }

    thread_local! {
        static CTX: RefCell<Ctx> = RefCell::new(Ctx::default());
    }

    fn proto_key(st: &mut State, ci: Ci) -> usize {
        let cl = st.ci_lclosure(ci).expect("a Lua frame");
        let p = cl.proto();
        &*p as *const rn_core::func::Proto as usize
    }

    /// `isblack(t)`: is the table's GC mark black?
    fn n_isblack(st: &mut State) -> LuaResult<usize> {
        let t = st.check_table(1)?;
        let p = t.as_ptr() as usize;
        // SAFETY: a live table of this state; the marked byte is at the same offset in every object header.
        let m = unsafe { *((p + table::MARKED) as *const u8) };
        st.push(Value::boolean(m & rn_core::gc::BLACKBIT != 0));
        Ok(1)
    }

    /// `isblackfn(f)`: is the closure's GC mark black? (BUNDLE-L: traversing a black closure marked its upvalues black)
    fn n_isblackfn(st: &mut State) -> LuaResult<usize> {
        let v = st.arg(0);
        let g = v.as_gc().expect("a function");
        // SAFETY: a live closure of this state; the marked byte is at the same offset in every object header.
        let m = unsafe { *((g as usize + table::MARKED) as *const u8) };
        st.push(Value::boolean(m & rn_core::gc::BLACKBIT != 0));
        Ok(1)
    }

    /// `grab()`: a no-op since PIa (the prototype handle comes from `cache::proto_ref`).
    fn n_grab(_st: &mut State) -> LuaResult<usize> {
        Ok(0) // PIa: `cache::proto_ref` replaced the grabbing (the scripts still call it)
    }

    fn start(st: &mut State, ci: Ci, id: TrId, pc: u32) -> bool {
        let key = proto_key(st, ci);
        // SAFETY: `key` is the address of the running closure's live `Proto`.
        let Some(pr) = c::proto_ref(unsafe { &*(key as *const rn_core::func::Proto) }) else {
            // the first calls run before the prototype is compiled: a transient refusal
            CTX.with(|c| c.borrow_mut().aborted.push("not compiled yet".into()));
            return false;
        };
        CTX.with(|c| c.borrow_mut().protos.insert(key, pr));
        let resolve: record::ResolveFn = Rc::new(|_st, p| {
            let pr = c::proto_ref(p)?;
            CTX.with(|c| {
                c.borrow_mut()
                    .protos
                    .insert(p as *const rn_core::func::Proto as usize, pr)
            });
            Some(pr)
        });
        let cfg = RecCfg {
            deps: Some(record::cache_hooks(id, resolve)),
            ..RecCfg::default()
        };
        match record::arm(st, ci, pc, cfg) {
            Some(tok) => {
                CTX.with(|c| c.borrow_mut().recs.insert(tok, (id, pr, key)));
                true
            }
            None => false,
        }
    }

    struct Res {
        addrs: HashMap<usize, Vec<Option<u64>>>,
    }

    impl Resolver for Res {
        fn proto(&self, key: ProtoKey) -> Option<ProtoInfoR> {
            // SAFETY: a key of the trace is the address of a live `Proto` (the dependency edges keep it alive).
            let p = unsafe { &*(key.0 as *const rn_core::func::Proto) };
            Some(ProtoInfoR {
                code: p.code_ptr(),
                k: p.k_ptr().cast(),
                maxstack: u16::from(p.maxstacksize),
            })
        }
        fn land_addr(&self, key: ProtoKey, pc: u32, _land: Land) -> Option<u64> {
            self.addrs.get(&key.0)?.get(pc as usize).copied().flatten()
        }
    }

    fn pipeline(st: &mut State, token: u64, rec: &Recorded) -> Result<(), String> {
        let (id, pr, root) = CTX
            .with(|c| c.borrow().recs.get(&token).copied())
            .ok_or("unknown token")?;
        let opts = CTX.with(|c| c.borrow().opts.clone());
        let ir = &rec.ir;
        let end: Vec<Ref> = rec.writeback.iter().map(|e| e.r).collect();
        let al = ra::allocate(
            ir,
            &end,
            &RaOpts {
                randra: opts.randra,
                reserve_gpr: vb_reg(),
                xregs: xregs(),
                ..RaOpts::default()
            },
        )
        .map_err(|e| format!("ra: {e:?}\n{}", rn_jit::tr::print::dump(ir)))?;
        let mut addrs = HashMap::new();
        for k in ir.deps.iter().map(|k| k.0).chain([root]) {
            // SAFETY: the keys are live compiled prototypes (the recorder registered edges and compiled the callees).
            let p = unsafe { &*(k as *const rn_core::func::Proto) };
            let pr = c::proto_ref(p).ok_or("dependency without a trace table")?;
            let v: Vec<Option<u64>> = c::with_tr(st, pr, |tp| {
                (0..p.code().len() as u32)
                    .map(|pc| tp.pc_addr(pc))
                    .collect()
            })
            .ok_or("no trace table")?;
            // the re-derived map (byte-compared with the placed code) is the cross-check of the cached one
            // SAFETY: as above.
            let want = unsafe { asm::support::tier1_pc_addrs(st, k) }?;
            if want != v {
                return Err(format!(
                    "TrProto::pc_addr differs from the re-derived map of {k:#x}"
                ));
            }
            addrs.insert(k, v);
        }
        let res = Res { addrs };
        let (snaps, descs) = exit::build_exits(ir, &al, &res)?;
        let counters = c::alloc_exit_counters(st, id, snaps.len()).ok_or("no exit counters")?;
        let pc = ir.head.1;
        let (head, pcl) = c::with_tr(st, pr, |tp| {
            let i = tp.head_index(pc).expect("head");
            (
                tp.head(i) as *const HeadData as u64,
                tp.head(i).pcl_addr.get() as u64,
            )
        })
        .ok_or("no trace table")?;
        let env = AsmEnv {
            seg_oracle: false,
            link_mat: false,
            trace_id: id.slot,
            counters: counters as u64,
            hot_exit: opts.hot_exit,
            shared_exit: 0,
            exit_handler: exit::jit_tr_exit as *const () as usize as u64,
            stack_off: thr::stack_ptr_offset(st) as i32,
            pcl_h: pcl,
            head,
            head_pc: pc,
            entry_fail_fn: c::jit_tr_entry_fail as *const () as usize as u64,
            calln: asm::jit_tr_calln as *const () as usize as u64,
            barrier: asm::jit_tr_barrier as *const () as usize as u64,
            alloc: asm::jit_tr_alloc as *const () as usize as u64,
            interrupt: jit_hooks::interrupt_flag_addr() as u64,
            exitstorm: opts.exitstorm,
            generic_only: opts.generic_only,
            count_iterations: true,
            budget_reg: vb_reg(),
            exit_cells: 0,
            side: None,
            vcell: 0,
            rec_sites: [(0, 0, 0); 6],
            rec_nsites: 0,
            rec_link_cell: 0,
            root_maxstack: 0,
            no_room_checks: false,
            side_room: false,
            frame_cells: false,
            lazy: false,
            prelude: false,
            lazy_leave_fn: 0,
            chain: false,
            root_code: 0,
            lazy_cs: None,
            lazy_nparams: 0,
            frame_link_snap: u32::MAX,
            setmt_inline: true,
            b2_arr: b2_knobs(),
            b2_tag: b2_knobs(),
            b2_hk: b2_knobs(),
            nopay: b4_knobs(),
            divmagic: b4_knobs(),
            hk_fuse: b4_knobs(),
            hk_key64: b4_knobs(),
            f2_byte: true,
            ccalls_inline: false,
            f2ifl: true,
            fibdiet: 0,
            lean_sub: true,
            libcall: 0,
            sink_stub: true,
            mat_fn: rn_jit::tr::exit::jit_tr_mat as *const () as usize as u64,
            budget_sub: false,
            poll_off: None,
            fpool: 0,
            fpool_off: 0,
        };
        let a = asm::assemble(ir, &al, &snaps, &rec.writeback, &env)
            .map_err(|e| format!("asm: {e:?}"))?;
        let em = rn_jit::x86::emit(&a.prog).map_err(|e| format!("x86: {e:?}"))?;
        let entry_off = em.label_off[a.entry as usize].ok_or("entry not bound")?;
        let text = format!("{}\n{}", rn_jit::tr::print::dump(ir), ra::dump(ir, &al));
        if std::env::var("P5B_DUMP").is_ok() {
            eprintln!("{text}");
            let _ = std::fs::write(std::env::temp_dir().join("trace-code.bin"), &em.code);
            eprintln!("entry_off {entry_off} len {}", em.code.len());
        }
        let newid = c::install(
            st,
            id,
            TraceInstall {
                code: em.code,
                entry_off,
                inner_off: 0,
                rec_ret: false,
                text: None,
                hash: 0,
                weight: 0,
            },
        )
        .map_err(|e| format!("install: {e:?}"))?;
        let meta = exit::ExitMeta {
            seg_oracle: 0,
            root_code: {
                // SAFETY: the root proto is live (running frame).
                unsafe { &*(root as *const rn_core::func::Proto) }.code_ptr()
            },
            snaps,
            exits: descs,
            vb: env.vb(),
            counters,
            cells: std::ptr::null(),
            peeled: ir.loop_at.is_some(),
            extent: ir.extent,
            depth: ir.depth,
            frame_cells: false,
            lazy: false,
        };
        c::attach_exits(st, newid, meta).ok_or("attach_exits")?;
        jit_hooks::rec_request_enter();
        CTX.with(|c| {
            let mut c = c.borrow_mut();
            c.installed += 1;
            c.dumps.push(text);
        });
        Ok(())
    }

    fn handle(st: &mut State, tok: u64, out: RecOutcome) {
        match out {
            RecOutcome::Closed(rec) => {
                if let Err(e) = pipeline(st, tok, &rec) {
                    CTX.with(|c| c.borrow_mut().failed.push(e.clone()));
                    if let Some((id, _, _)) = CTX.with(|c| c.borrow().recs.get(&tok).copied()) {
                        c::abort_recording(st, id, "TestPipeline", false);
                    }
                }
            }
            RecOutcome::Aborted(why) => {
                let Some((id, _, _)) = CTX.with(|c| c.borrow().recs.get(&tok).copied()) else {
                    return;
                };
                CTX.with(|c| c.borrow_mut().aborted.push(format!("{why:?}")));
                match why {
                    AbortReason::Unstable => c::abort_unstable(st, id),
                    w => {
                        let s: &'static str = keep(Box::leak(format!("{w:?}").into_boxed_str()));
                        c::abort_recording(st, id, s, false);
                    }
                }
            }
        }
    }

    fn e2e_step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
        let r = record::step(st, ci, pc, token);
        if r == RecStep::Leave {
            if let Some((tok, out)) = record::take_outcome() {
                handle(st, tok, out);
            }
        }
        r
    }

    fn e2e_abort(st: &mut State, why: Abort, token: u64) {
        record::abort(st, why, token);
        if let Some((tok, out)) = record::take_outcome() {
            handle(st, tok, out);
        }
    }

    fn init() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            assert!(
                rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
                "the compiler must be installed by this test binary"
            );
            assert!(jit_hooks::install_rec(RecVTable {
                step: e2e_step,
                abort: e2e_abort
            }));
            #[cfg(feature = "tr_mutants")]
            rn_jit::tr::mutants::apply_vm_mutants();
        });
    }

    pub fn new_state(traced: Option<&Opts>) -> LuaState {
        init();
        let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
        rn_jit::set_t15(
            &mut st,
            rn_jit::t15::runtime::T15Config::for_mode(rn_jit::t15::runtime::T15Mode::Off),
        );
        match traced {
            Some(o) => {
                rn_jit::set_mode(&mut st, Mode::Always);
                let mut cfg = TrConfig::for_mode(TrMode::On);
                cfg.hot_loop = o.hot_loop;
                cfg.hot_exit = o.hot_exit;
                cfg.opts.tracemove = o.tracemove;
                if std::env::var("P5B_VERBOSE").is_ok() {
                    cfg.verbose = 2;
                }
                rn_jit::rt::set_trace(&mut st, cfg);
                c::set_starter(&mut st, Some(start));
                if let Some(b) = o.count_budget {
                    rn_jit::set_count_budget(&mut st, b);
                }
                CTX.with(|c| {
                    let mut c = c.borrow_mut();
                    *c = Ctx::default();
                    c.opts = o.clone();
                });
            }
            None => rn_jit::set_mode(&mut st, Mode::Off),
        }
        let g = st.globals();
        st.raw_set_str(g, b"grab", Value::native(n_grab)).unwrap();
        st.raw_set_str(g, b"isblack", Value::native(n_isblack))
            .unwrap();
        st.raw_set_str(g, b"isblackfn", Value::native(n_isblackfn))
            .unwrap();
        st
    }

    pub fn exec(st: &mut LuaState, src: &str) -> String {
        let base = st.top();
        assert_eq!(
            st.load_buffer(src.as_bytes(), b"=snippet", None),
            Status::Ok,
            "load"
        );
        let func = st.top() - 1;
        let status = st.pcall(func, LUA_MULTRET, 0);
        let v = st.get(func);
        let out = v.as_string().map_or_else(
            || format!("{v:?}"),
            |s| String::from_utf8_lossy(s.as_bytes()).into_owned(),
        );
        st.set_top(base);
        format!("{status:?}:{out}")
    }

    /// What a traced run did.
    #[derive(Debug, Default)]
    pub struct Report {
        pub installed: u32,
        pub aborted: Vec<String>,
        pub failed: Vec<String>,
        pub dumps: Vec<String>,
        pub entries: u64,
        pub entry_fails: u64,
        pub iterations: u64,
        pub stats: c::TrStats,
    }

    /// Runs `src` in a traced state; the report says what the pipeline did.
    pub fn run_traced(src: &str, o: &Opts) -> (String, Report) {
        let mut st = new_state(Some(o));
        let out = exec(&mut st, src);
        let (mut entries, mut entry_fails) = (0, 0);
        let prs: Vec<ProtoRef> = CTX.with(|c| c.borrow().protos.values().copied().collect());
        for pr in prs {
            for h in c::head_infos(&mut st, pr) {
                entries += u64::from(h.entry_ok);
                entry_fails += u64::from(h.entry_fail);
            }
        }
        let stats = c::tr_stats(&mut st);
        // SAFETY: the area follows the budget word of the live per-state data.
        let iterations = unsafe {
            (*rn_jit::rt::trace_budget_ptr(&mut st)
                .cast::<u8>()
                .add(exit_area::FROM_R15 as usize)
                .cast::<ExitArea>())
            .spill[asm::ITER_SLOT as usize]
        };
        let r = CTX.with(|c| {
            let c = c.borrow();
            Report {
                installed: c.installed,
                aborted: c.aborted.clone(),
                failed: c.failed.clone(),
                dumps: c.dumps.clone(),
                entries,
                entry_fails,
                iterations,
                stats,
            }
        });
        drop(st);
        record::discard();
        (out, r)
    }

    /// The baseline: no compiler, no traces.
    pub fn run_plain(src: &str) -> String {
        let mut st = new_state(None);
        let out = exec(&mut st, src);
        drop(st);
        out
    }

    /// The Lua library shared by the programs: result formatting and a deterministic random generator.
    pub const PRELUDE: &str = r##"
local seed = 12345
function rnd(n) seed = (seed * 1103515245 + 12345) % 2147483648; return (seed >> 8) % n end
local function one(v)
  local t = type(v)
  if t == "number" then
    if v ~= v then return string.format("%.17g", v) end  -- keeps the NaN sign: "nan" and "-nan" differ
    return math.type(v) .. ":" .. string.format("%.17g", v)
  elseif t == "table" then
    local n, s, h = 0, 0, 0
    for i = 1, 64 do
      local e = v[i]
      if e ~= nil then n = n + 1 end
      local te = type(e)
      -- the type of every element (integer vs float included), not only the sum
      local code = 0
      if te == "number" then
        s = s + (e == e and e or 0)
        code = (math.type(e) == "integer") and 1 or 2
      elseif te == "table" then s = s + (e.id or 0); code = 3
      elseif te == "string" then s = s + #e + e:byte(1); code = 4
      elseif te == "boolean" then s = s + (e and 7 or 3); code = e and 5 or 6 end
      h = (h * 7 + code) % 1000003
    end
    return "tab[" .. n .. "," .. string.format("%.17g", s) .. "," .. h .. "]"
  end
  return tostring(v)
end
function fmt(ok, ...)
  local t = { tostring(ok) }
  for i = 1, select("#", ...) do t[#t + 1] = one((select(i, ...))) end
  return table.concat(t, ",")
end
"##;

    /// A program: its Lua source (defining `f`, `gen`, optionally `warm`) and how many calls the driver makes.
    pub struct Prog {
        pub name: &'static str,
        pub src: &'static str,
        pub calls: u32,
    }

    pub fn full_source(p: &Prog, gc_between: bool) -> String {
        format!(
            "{PRELUDE}\n{}\nlocal out = {{}}\nif warm then warm() end\nfor k = 1, {} do\n  GRAB = (k <= 60)\n  out[#out + 1] = fmt(pcall(f, gen(k)))\n  {}\nend\nreturn table.concat(out, \"\\n\")\n",
            p.src,
            p.calls,
            if gc_between {
                "local junk = {} for j = 1, 30 do junk[j] = {j} end collectgarbage('step', 20)"
            } else {
                ""
            }
        )
    }

    pub const PROGS: &[Prog] = &[
        Prog {
            name: "sum_int_and_float_entry",
            src: r#"
function f(n, x)
  if GRAB then grab() end
  local s = x
  for i = 1, n do s = s + i end
  return s
end
function gen(k)
  local x = (k % 5 == 0) and 0.5 or ((k % 7 == 0) and 9007199254740993 or 0)
  return rnd(200), x
end
"#,
            calls: 120,
        },
        Prog {
            name: "float_accumulate_with_division",
            src: r#"
function f(n, x, y)
  if GRAB then grab() end
  local s, t = x, y
  for i = 1, n do s = s + i * 0.5; t = t / 2 + s - t * 0.25 end
  return s, t
end
function gen(k)
  local specials = { 0.0, -0.0, 1 / 0, -1 / 0, 0 / 0, 1e300, 5e-324 }
  local x = (k % 3 == 0) and specials[rnd(#specials) + 1] or rnd(100) / 7
  return rnd(120), x, rnd(50) / 3
end
"#,
            calls: 80,
        },
        Prog {
            name: "array_read_write_int_and_float",
            src: r#"
local arrs = {}
function warm()
  for j = 1, 4 do
    local a = {}
    for i = 1, 64 do a[i] = (j % 2 == 0) and i or i + 0.5 end
    arrs[j] = a
  end
end
function f(j, n, k)
  if GRAB then grab() end
  local a = arrs[j]
  local s = 0
  for i = 1, n do
    local v = a[i]
    s = s + v
    a[i] = v + k
  end
  return s, a
end
function gen(k) return rnd(4) + 1, rnd(70), (k % 4 == 0) and 0.25 or 1 end
"#,
            calls: 160,
        },
        Prog {
            name: "branches_and_integer_division",
            src: r#"
function f(n, m)
  if GRAB then grab() end
  local s = 1
  for i = 1, n do
    if i % 3 == 0 then s = s + (i // m) elseif i % 5 == 0 then s = s * 3 % 1000003 else s = s ~ (i << 3) end
    s = s + (s >> 2) - (i % 7)
  end
  return s
end
function gen(k) return rnd(150), (k % 9 == 0) and 0 or (rnd(20) - 10) end
"#,
            calls: 120,
        },
        Prog {
            name: "inlined_call_and_callee_identity",
            src: r#"
local function g(x, gr) if gr then grab() end return x * 2 + 1 end
local function h(x) return x * 3 - 1 end
local fs = { g, g, h }
function warm() for j = 1, 12 do g(j, true) end end
function f(n, mix)
  if GRAB then grab() end
  local s = 0
  for i = 1, n do
    local fn = mix and fs[(i % 3) + 1] or g
    s = s + fn(i)
  end
  return s
end
function gen(k) return rnd(100), k % 4 == 0 end
"#,
            calls: 100,
        },
        Prog {
            name: "nested_inlined_calls_with_guards",
            src: r#"
local function k(x, gr) if gr then grab() end if x % 4 == 0 then return x + 1 end return x - 1 end
local function g(x, gr) if gr then grab() end local y = k(x) if y > 50 then return y - 1 end return y * 2 end
function warm() for j = 1, 12 do g(j, true); k(j, true) end end
function f(n, lim)
  if GRAB then grab() end
  local s = 0
  for i = 1, n do
    local v = g(i)
    if v < lim then s = s + v else s = s - 1 end
  end
  return s
end
function gen(k) return rnd(120), rnd(100) end
"#,
            calls: 100,
        },
        Prog {
            name: "intrinsics",
            src: r#"
function f(n, x)
  if GRAB then grab() end
  local sqrt, floor, abs, max, min = math.sqrt, math.floor, math.abs, math.max, math.min
  local s = 0
  for i = 1, n do
    s = s + floor(sqrt(i) * 3) + abs(i - n // 2) + max(i, 7) + min(x, i)
  end
  return s
end
function gen(k) return rnd(100), (k % 6 == 0) and 2.5 or rnd(50) end
"#,
            calls: 100,
        },
        Prog {
            name: "upvalues",
            src: r#"
local c = 0
local d = 1.5
function f(n, k)
  if GRAB then grab() end
  for i = 1, n do c = c + i; d = d + k end
  return c, d
end
function gen(k) return rnd(100), (k % 5 == 0) and 1 or 0.5 end
"#,
            calls: 100,
        },
        Prog {
            name: "while_and_float_for",
            src: r#"
function f(n, step)
  if GRAB then grab() end
  local i, s = 0, 0.0
  while i < n do s = s + i; i = i + 1 end
  for x = 0.0, 1.0, step do s = s + x end
  return s
end
function gen(k) return rnd(100), 0.125 * (rnd(3) + 1) end
"#,
            calls: 80,
        },
        Prog {
            name: "table_stores_of_collectables",
            src: r#"
local objs = {}
for i = 1, 3 do objs[i] = { id = i * 10 } end
function f(t, n)
  if GRAB then grab() end
  for i = 1, n do t[i] = objs[(i % 3) + 1] end
  return t
end
function gen(k)
  local t = {}
  for i = 1, 40 do t[i] = objs[1] end
  return t, rnd(40)
end
"#,
            calls: 100,
        },
        Prog {
            name: "string_boolean_and_nil_stores",
            src: r#"
function f(t, n)
  if GRAB then grab() end
  for i = 1, n do
    local r = i % 3
    if r == 0 then t[i] = nil elseif r == 1 then t[i] = "odd" else t[i] = true end
  end
  return t
end
function gen(k)
  local t = {}
  for i = 1, 48 do t[i] = (k % 2 == 0) and i or "x" end
  return t, rnd(60)
end
"#,
            calls: 80,
        },
        Prog {
            name: "nested_loops",
            src: r#"
function f(n, m)
  if GRAB then grab() end
  local s = 0
  for i = 1, n do
    for j = 1, m do s = s + i * j end
  end
  return s
end
function gen(k) return rnd(20), rnd(40) end
"#,
            calls: 80,
        },
        Prog {
            name: "mod_and_shift_edge_cases",
            src: r#"
function f(n, a, b)
  if GRAB then grab() end
  local s = 0
  for i = 1, n do
    s = s + (a % (b + i - 5)) + (i << (a & 63)) + (-a // (b - i + 3)) + (a >> (i % 70))
  end
  return s
end
function gen(k)
  local edge = { 0, 1, -1, 7, -7, math.maxinteger, math.mininteger, 1 << 62 }
  return rnd(12), edge[rnd(#edge) + 1], rnd(11) - 5
end
"#,
            calls: 160,
        },
    ];
}

#[cfg(not(miri))]
use e2e::{Opts, Prog};

#[cfg(not(miri))]
fn check_prog(p: &Prog, o: &Opts) -> e2e::Report {
    eprintln!("check_prog {} {o:?}", p.name);
    let src = e2e::full_source(p, o.gc_between);
    let want = e2e::run_plain(&src);
    let (got, rep) = e2e::run_traced(&src, o);
    assert!(
        rep.failed.is_empty(),
        "{}: pipeline failures {:?}",
        p.name,
        rep.failed
    );
    if got != want {
        let (g, w): (Vec<&str>, Vec<&str>) = (got.lines().collect(), want.lines().collect());
        let at = g
            .iter()
            .zip(&w)
            .position(|(a, b)| a != b)
            .unwrap_or(g.len().min(w.len()));
        panic!(
            "{} with {o:?}: result differs at call {at}\n got  {:?}\n want {:?}\n{}",
            p.name,
            g.get(at),
            w.get(at),
            rep.dumps.first().cloned().unwrap_or_default()
        );
    }
    rep
}

#[cfg(not(miri))]
#[test]
fn e2e_traces_run_and_equal_the_interpreter() {
    both(|| {
        let mut total_entries = 0;
        for p in e2e::PROGS {
            let rep = check_prog(p, &Opts::default());
            eprintln!(
            "{:<36} installed {} entries {} iterations {} entry_fails {} stats started {} flushes {} rerec {} aborts {:?}",
            p.name, rep.installed, rep.entries, rep.iterations, rep.entry_fails, rep.stats.started, rep.stats.flushes, rep.stats.rerec, rep.stats.aborted
        );
            total_entries += rep.entries;
            assert!(
                rep.installed >= 1,
                "{}: no trace was installed ({:?})",
                p.name,
                rep.aborted
            );
            assert!(rep.entries >= 1, "{}: no trace was entered", p.name);
            assert!(
                rep.iterations >= rep.entries,
                "{}: traces ran no iterations",
                p.name
            );
        }
        assert!(total_entries > 100);
    });
}

#[cfg(not(miri))]
#[test]
#[ignore]
fn e2e_one_config() {
    both(|| {
        let name =
            std::env::var("P5B_PROG").unwrap_or_else(|_| "inlined_call_and_callee_identity".into());
        let storm = std::env::var("P5B_STORM").ok().and_then(|v| v.parse().ok());
        let p = e2e::PROGS.iter().find(|p| p.name == name).unwrap();
        let o = Opts {
            exitstorm: storm,
            ..Opts::default()
        };
        check_prog(p, &o);
    });
}

#[cfg(not(miri))]
#[test]
#[ignore]
fn e2e_dump_one() {
    let name =
        std::env::var("P5B_PROG").unwrap_or_else(|_| "inlined_call_and_callee_identity".into());
    let p = e2e::PROGS.iter().find(|p| p.name == name).expect("program");
    let rep = check_prog(p, &Opts::default());
    for d in &rep.dumps {
        eprintln!("{d}\n=========");
    }
}

#[cfg(not(miri))]
#[test]
fn e2e_exitstorm_every_guard_fails_often_and_results_stay_exact() {
    both(|| {
        for p in e2e::PROGS {
            for n in [1u32, 2, 3, 7, 31] {
                let o = Opts {
                    exitstorm: Some(n),
                    ..Opts::default()
                };
                let rep = check_prog(p, &o);
                assert!(
                    rep.installed >= 1,
                    "{} storm {n}: nothing installed",
                    p.name
                );
            }
        }
    });
}

#[cfg(not(miri))]
#[test]
fn e2e_generic_stubs_only_go_through_the_shared_routine_and_the_rust_restore() {
    both(|| {
        for p in e2e::PROGS {
            for storm in [None, Some(3)] {
                let o = Opts {
                    generic_only: true,
                    exitstorm: storm,
                    ..Opts::default()
                };
                let rep = check_prog(p, &o);
                assert!(rep.iterations >= 1, "{}: no trace ran", p.name);
            }
        }
    });
}

#[cfg(not(miri))]
#[test]
fn e2e_random_register_allocation() {
    both(|| {
        for p in e2e::PROGS {
            for seed in 1..=6u64 {
                let o = Opts {
                    randra: Some(seed * 7919),
                    exitstorm: (seed % 3 == 0).then_some(5),
                    ..Opts::default()
                };
                check_prog(p, &o);
            }
        }
    });
}

#[cfg(not(miri))]
#[test]
fn e2e_small_count_budgets_exhaust_inside_traces() {
    both(|| {
        for p in e2e::PROGS {
            for budget in [7, 23, 64, 301, 5000] {
                let o = Opts {
                    count_budget: Some(budget),
                    ..Opts::default()
                };
                check_prog(p, &o);
            }
        }
    });
}

#[cfg(not(miri))]
#[test]
fn e2e_hot_exits_flush_and_rerecord_and_gc_runs_between_calls() {
    both(|| {
        let mut flushed = 0;
        for p in e2e::PROGS {
            for (hot_exit, hot_loop) in [(2u32, 2i32), (3, 1), (1, 8)] {
                let o = Opts {
                    hot_exit,
                    hot_loop,
                    gc_between: true,
                    ..Opts::default()
                };
                let rep = check_prog(p, &o);
                flushed += rep.stats.exit_type_hot + rep.stats.exit_other_hot;
            }
        }
        assert!(flushed > 0, "no exit ever got hot");
    });
}

#[cfg(not(miri))]
#[test]
fn e2e_a_long_loop_runs_inside_the_trace() {
    both(|| {
        let p = Prog {
            name: "long_sum",
            src: r#"
function f(n)
  if GRAB then grab() end
  local s = 0
  for i = 1, n do s = s + i * 3 - (i >> 1) end
  return s
end
function gen(k) return 200000 + k end
"#,
            calls: 6,
        };
        let rep = check_prog(&p, &Opts::default());
        assert!(rep.iterations > 600_000, "iterations {}", rep.iterations);
    });
}

/// Programs for the review findings of P5b (P5c): NaN sign, coroutines, hooks, shifts.
#[cfg(not(miri))]
const P5C_PROGS: &[Prog] = &[
    Prog {
        name: "nan_sign_through_pow_and_fmod",
        src: r#"
function f(k)
  local n = -(0 / 0)
  local m = 0 / 0
  local r, q, r2, q2
  for i = 1, 200 do r = n ^ 3; q = n % 2.5 end
  for i = 1, 200 do r2 = m ^ 3; q2 = m % 2.5 end
  return r, q, r2, q2
end
function gen(k) return k end
"#,
        calls: 12,
    },
    Prog {
        name: "coroutines_yield_between_trace_entries",
        src: r#"
function f(n, m)
  local co = coroutine.wrap(function()
    local s = 0
    for r = 1, m do
      for i = 1, n do s = s + i * r end
      coroutine.yield(s)
    end
    return s
  end)
  local last, main = 0, 0
  for r = 1, m + 1 do last = co(); for i = 1, n do main = main + i end end
  return last, main
end
function gen(k) return rnd(40) + 1, rnd(5) + 1 end
"#,
        calls: 80,
    },
    Prog {
        name: "hooks_installed_while_a_trace_is_hot",
        src: r#"
local cnt = 0
function f(n, mode)
  if mode == 1 then debug.sethook(function() cnt = cnt + 1 end, "", 7)
  elseif mode == 2 then debug.sethook(function() cnt = cnt + 1 end, "l")
  elseif mode == 3 then debug.sethook() end
  local s = 0
  for i = 1, n do s = s + i end
  return s, cnt
end
function gen(k)
  local m = k % 11
  return 20 + rnd(50), (m == 3) and 1 or ((m == 6) and 2 or ((m == 9) and 3 or 0))
end
"#,
        calls: 120,
    },
    Prog {
        name: "variable_negative_and_large_shifts",
        src: r#"
function f(n, x, a, b)
  local s = 0
  for i = 1, n do
    local c = (i * 37) % 134 - 67
    s = s ~ (x << a) ~ (x >> b) ~ (x << c) ~ (x >> c) ~ ((i << c) + (i >> b))
  end
  return s
end
function gen(k)
  local edge = { 0, 1, -1, 7, -7, math.maxinteger, math.mininteger, 1 << 62 }
  local sh = { -3, -64, 64, 65, math.mininteger, math.maxinteger, 63, 0 }
  return rnd(40), edge[rnd(#edge) + 1], sh[rnd(#sh) + 1], sh[rnd(#sh) + 1]
end
"#,
        calls: 160,
    },
    Prog {
        name: "stack_grows_between_entries",
        src: r#"
local function deep(d) if d == 0 then return 0 end return 1 + deep(d - 1) end
function f(n, d)
  local s = deep(d)
  for i = 1, n do s = s + i end
  return s
end
function gen(k) return rnd(60), (k % 5 == 0) and 3000 or rnd(20) end
"#,
        calls: 100,
    },
];

#[cfg(not(miri))]
#[test]
fn e2e_nan_sign_survives_pow_and_fmod_in_traces() {
    both(|| {
        let p = &P5C_PROGS[0];
        let rep = check_prog(p, &Opts::default());
        assert!(rep.entries >= 1, "no trace was entered");
        // the interpreter's output itself: `-(0/0) ^ 3` and `% 2.5` keep the NaN's sign (`nan`), `(0/0)` is `-nan`
        let want = e2e::run_plain(&e2e::full_source(p, false));
        assert!(
            want.contains("true,nan,nan,nan,-nan"),
            "reference output: {want}"
        );
        let o = Opts {
            generic_only: true,
            ..Opts::default()
        };
        check_prog(p, &o);
    });
}

#[cfg(not(miri))]
#[test]
fn e2e_coroutines_hooks_shifts_and_stack_moves() {
    both(|| {
        for p in &P5C_PROGS[1..4] {
            for o in [
                Opts::default(),
                Opts {
                    generic_only: true,
                    exitstorm: Some(7),
                    ..Opts::default()
                },
            ] {
                let rep = check_prog(p, &o);
                assert!(rep.installed >= 1, "{}: no trace was installed", p.name);
                if o.exitstorm.is_none() {
                    assert!(rep.entries >= 1, "{}: no trace was entered", p.name);
                }
            }
        }
    });
}

#[cfg(not(miri))]
#[test]
fn e2e_tracemove_moves_the_stack_before_every_entry() {
    both(|| {
        let (mut moves, mut entries) = (0, 0);
        for p in e2e::PROGS.iter().chain(&P5C_PROGS[1..]) {
            let o = Opts {
                tracemove: true,
                ..Opts::default()
            };
            let rep = check_prog(p, &o);
            moves += rep.stats.moves;
            entries += rep.entries;
        }
        assert!(
            entries > 100 && moves > 100,
            "entries {entries} moves {moves}"
        );
    });
}

#[cfg(not(miri))]
thread_local! {
    static HANDLED: Cell<u32> = const { Cell::new(0) };
}

#[cfg(not(miri))]
fn count_handler(_st: &mut rn_core::state::State, _id: rn_jit::tr::cache::TrId) {
    HANDLED.with(|h| h.set(h.get() + 1));
}

/// P5b review finding 2a: the hot divert fires once. A loop-end ("Other") exit site that is taken thousands of times
/// goes through the Rust handler only until it reports hot; afterwards the inline path counts on.
#[cfg(not(miri))]
#[test]
fn e2e_a_loop_end_exit_site_diverts_once_and_stays_inline() {
    both(|| {
        let p = Prog {
            name: "many_short_loops",
            src: r#"
function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
function gen(k) return 7 end
"#,
            calls: 3000,
        };
        HANDLED.with(|h| h.set(0));
        rn_jit::tr::exit::set_probe(Some(count_handler));
        let rep = check_prog(&p, &Opts::default());
        rn_jit::tr::exit::set_probe(None);
        let calls = HANDLED.with(Cell::get);
        assert!(rep.entries > 2500, "entries {}", rep.entries);
        assert!(
            calls > 0 && calls < 100,
            "the handler ran {calls} times for {} entries: the divert must not stay on",
            rep.entries
        );
    });
}

/// P5b review finding 2b: an entry check that fails on every entry flushes the trace (3.3), through the inline stub
/// and through the generic one (`generic_only`).
#[cfg(not(miri))]
#[test]
fn e2e_an_eqref_entry_check_failing_on_every_entry_flushes_the_trace() {
    both(|| {
        let p = Prog {
            name: "callee_changes_for_good",
            src: r#"
local function h1(x) return x + 1 end
local function h2(x) return x + 2 end
function f(fn, n)
  local s = 0
  for i = 1, n do s = s + fn(i) end
  return s
end
function gen(k) return (k <= 12) and h1 or h2, 9 end
"#,
            calls: 200,
        };
        for generic_only in [false, true] {
            let o = Opts {
                generic_only,
                // no exit ever gets hot: only the entry-failure threshold can flush
                hot_exit: 1 << 30,
                ..Opts::default()
            };
            let rep = check_prog(&p, &o);
            assert_eq!(rep.stats.exit_type_hot + rep.stats.exit_other_hot, 0);
            assert!(
                rep.stats.flushes >= 1 && rep.stats.rerec >= 1,
                "generic_only {generic_only}: the persistent entry failure never flushed ({:?})",
                rep.stats
            );
        }
    });
}

/// `a[i] = b[i]; b[i] = false` in a trace while `a` is black and the value white (the only reference moves to `a`):
/// a skipped barrier loses the value in the rest of the collection cycle.
#[cfg(not(miri))]
fn tbar_prog(mode: &'static str) -> &'static Prog {
    let src = format!(
        r#"
local N = 24
local MODE = "{mode}"
if MODE == "gen" then collectgarbage("generational") else collectgarbage("incremental") end
-- 5.4: one 'step 0' runs a whole small cycle (stepsize * stepmul work); with a step size of 2 bytes it is a single
-- collector step, so A is observed black in the middle of the cycle as in 5.5
if MODE == "inc" and _VERSION == "Lua 5.4" then collectgarbage("incremental", 0, 0, 1) end
A = {{}}; B = {{}}
local function move(a, b, n) for i = 1, n do a[i] = b[i]; b[i] = false end end
function f(k)
  for i = 1, N do A[i] = false; B[i] = false end
  collectgarbage()
  collectgarbage("stop")
  if MODE == "inc" then
    local guard = 0
    while not isblack(A) do
      collectgarbage("step", 0)
      guard = guard + 1
      assert(guard < 100000, "A never turned black")
    end
  end
  for i = 1, N do B[i] = {{ id = k * 100 + i }} end   -- white objects, born after A was blackened
  local was = isblack(A)
  move(A, B, N)
  local now = isblack(A)
  if MODE == "inc" then
    local guard = 0
    repeat guard = guard + 1; assert(guard < 100000) until collectgarbage("step", 0) and guard > 0
  else
    collectgarbage("step", 0)
  end
  for j = 1, 3000 do local t = {{ j, j, j }} end        -- reuse freed memory
  local sum = 0
  for i = 1, N do sum = sum + A[i].id end
  collectgarbage("restart")
  return was, now, sum
end
function gen(k) return k end
"#
    );
    keep(Box::leak(Box::new(Prog {
        name: if mode == "gen" {
            "tbar_gen"
        } else {
            "tbar_inc"
        },
        src: keep(Box::leak(src.into_boxed_str())),
        calls: 40,
    })))
}

#[cfg(not(miri))]
#[test]
fn e2e_tbar_in_an_incremental_gc_cycle() {
    both(|| {
        let p = tbar_prog("inc");
        let rep = check_prog(p, &Opts::default());
        assert!(
            rep.entries >= 1,
            "no trace was entered (installed {}, aborted {:?}, failed {:?})",
            rep.installed,
            rep.aborted,
            rep.failed
        );
        let out = e2e::run_plain(&e2e::full_source(p, false));
        assert!(
            out.trim_start_matches("Ok:")
                .lines()
                .all(|l| l.starts_with("true,true,false,")),
            "{out}"
        );
    });
}

#[cfg(not(miri))]
#[test]
fn e2e_tbar_in_a_generational_gc_cycle() {
    both(|| {
        let p = tbar_prog("gen");
        let rep = check_prog(p, &Opts::default());
        assert!(rep.entries >= 1, "no trace was entered");
        let out = e2e::run_plain(&e2e::full_source(p, false));
        assert!(
            out.trim_start_matches("Ok:")
                .lines()
                .all(|l| l.starts_with("true,true,false,")),
            "{out}"
        );
    });
}

/// F1: `a.slot = pool[i]; pool[i] = false` in a trace while `a` is black and the value white: the `TBAR` after the
/// `HSTORE` is what keeps the value alive (mutant `hstore_tbar_skip` loses it).
#[cfg(not(miri))]
fn tbar_fields_prog(mode: &'static str) -> &'static Prog {
    let src = format!(
        r#"
local N = 24
local MODE = "{mode}"
if MODE == "gen" then collectgarbage("generational") else collectgarbage("incremental") end
if MODE == "inc" and _VERSION == "Lua 5.4" then collectgarbage("incremental", 0, 0, 1) end
A = {{slot = false}}; POOL = {{}}
local function move(a, pool, n) for i = 1, n do a.slot = pool[i]; pool[i] = false end end
function f(k)
  for i = 1, N do POOL[i] = false end
  A.slot = false
  collectgarbage()
  collectgarbage("stop")
  if MODE == "inc" then
    local guard = 0
    while not isblack(A) do
      collectgarbage("step", 0)
      guard = guard + 1
      assert(guard < 100000, "A never turned black")
    end
  end
  for i = 1, N do POOL[i] = {{ id = k * 100 + i }} end
  local was = isblack(A)
  move(A, POOL, N)
  local now = isblack(A)
  if MODE == "inc" then
    local guard = 0
    repeat guard = guard + 1; assert(guard < 100000) until collectgarbage("step", 0) and guard > 0
  else
    collectgarbage("step", 0)
  end
  for j = 1, 3000 do local t = {{ j, j, j }} end
  local sum = A.slot.id
  collectgarbage("restart")
  return was, now, sum
end
function gen(k) return k end
"#
    );
    keep(Box::leak(Box::new(Prog {
        name: if mode == "gen" {
            "tbar_fields_gen"
        } else {
            "tbar_fields_inc"
        },
        src: keep(Box::leak(src.into_boxed_str())),
        calls: 40,
    })))
}

#[cfg(not(miri))]
#[test]
fn e2e_tbar_after_hstore_in_gc_cycles() {
    both(|| {
        for mode in ["inc", "gen"] {
            let p = tbar_fields_prog(mode);
            let rep = check_prog(p, &Opts::default());
            assert!(
                rep.entries >= 1,
                "{mode}: no trace was entered (installed {}, aborted {:?}, failed {:?})",
                rep.installed,
                rep.aborted,
                rep.failed
            );
            let out = e2e::run_plain(&e2e::full_source(p, false));
            assert!(
                out.trim_start_matches("Ok:")
                    .lines()
                    .all(|l| l.starts_with("true,true,false,")),
                "{mode}: {out}"
            );
        }
    });
}

#[test]
fn hand_typechecked_uload_with_exitstorm_keeps_its_address() {
    both(|| {
        // The storm check of a guard uses V7: it must not sit between the computation of the upvalue's value address
        // (also in V7) and the typecheck that reads through it.
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(0, &[]);
        let cl = b.ins(TOp::Sload, Ty::Lcf, 0, 0xFFFF, NOREF, 0);
        let k1 = b.kint(1);
        let uref = b.ins(TOp::Uref, Ty::P64, 0, cl, k1, 0);
        let u = b.ins(TOp::Uload, Ty::Int, F_GUARD, uref, NOREF, guard_aux(s1, 1));
        let lim = b.kint(1000);
        b.ins(TOp::Lt, Ty::Int, 0, u, lim, guard_aux(s0, 0));
        for (cell_t, cell_v, counter, want) in [
            (Ty::Int, 5i64, 2u64, 0u32), // ULOAD passes (counter 2 -> 1), the compare passes the storm? no: it fires (1 -> 0)
            (Ty::Int, 5, 3, 3), // no storm exit at all: the compare holds, the rolled trace loops (not run)
            (Ty::Num, 5, 9, 1), // the typecheck fails for real
        ] {
            if want == 3 {
                continue;
            }
            let mut m = model_ints(&[]);
            m.closures[0][1] = Val {
                t: cell_t,
                bits: cell_v as u64,
            };
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            let nspill = {
                let env = base_env(&mut w);
                let res = TestRes::linear(b.ir.snaps.len());
                let (_, al, _) = compile(&b.ir, &[], None, &env, &res).unwrap();
                al.nspill
            };
            w.set_spill(usize::from(nspill) + 1, counter);
            let (t, _, _, _) = exec(&b.ir, &mut w, |e| e.exitstorm = Some(1000));
            assert_eq!(snap_of(t), Some(want), "cell {cell_t:?} counter {counter}");
        }
    });
}

/// Every mutant of this task must make its named test(s) FAIL (child process with `RUSTYNAIL_JIT_MUT` set) while the
/// same tests pass in a baseline child. (The native test is not used here: a mutant that removes the budget check
/// makes a native loop run forever.)
#[cfg(all(feature = "tr_mutants", not(miri)))]
#[test]
fn mutants_are_caught() {
    if std::env::var("TR_ASM_CHILD").is_ok() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let cases = [
        (
            "budget_no_check",
            "hand_budget_charge_and_exit_value",
            "mem",
        ),
        (
            "budget_no_check",
            "hand_budget_charge_and_exit_value",
            "reg",
        ),
        ("budget_no_check", "emu_peeled_matches_interp", "mem"),
        ("budget_no_check", "emu_peeled_matches_interp", "reg"),
        (
            "budget_no_store",
            "hand_budget_charge_and_exit_value",
            "mem",
        ),
        (
            "budget_no_store",
            "hand_budget_charge_and_exit_value",
            "reg",
        ),
        ("budget_no_store", "emu_rolled_matches_interp", "mem"),
        ("budget_no_store", "emu_rolled_matches_interp", "reg"),
        ("entry_no_extent", "hand_entry_checks_tr6", "both"),
        ("entry_no_ci_spare", "hand_entry_checks_tr6", "both"),
        ("entry_no_typecheck", "emu_peeled_matches_interp", "both"),
        (
            "entry_no_typecheck",
            "hand_entry_failure_bookkeeping_follows_the_free_peel_rule",
            "both",
        ),
        ("poll_skip", "hand_poll_trap_and_interrupt", "both"),
        ("poll_skip", "emu_peeled_matches_interp", "both"),
        ("fcmp_negated", "emu_peeled_matches_interp", "both"),
        (
            "tbar_skip",
            "hand_tbar_calls_the_barrier_exactly_for_black_tables_and_white_values",
            "both",
        ),
        ("tbar_skip", "e2e_tbar_in_an_incremental_gc_cycle", "both"),
        ("tbar_skip", "e2e_tbar_in_a_generational_gc_cycle", "both"),
        (
            "entry_no_typecheck",
            "hand_rolled_early_typecheck_in_iteration_two_is_not_an_entry_failure",
            "both",
        ),
        ("rolled_no_writeback", "emu_rolled_matches_interp", "both"),
        (
            "rolled_no_writeback",
            "e2e_traces_run_and_equal_the_interpreter",
            "both",
        ),
    ];
    for (mutant, test, vb) in cases {
        for m in ["", mutant] {
            let (ok, out) = run_child(&exe, test, m, vb);
            if m.is_empty() {
                assert!(ok, "baseline child {test} failed:\n{out}");
            } else {
                assert!(!ok, "mutant {mutant} was NOT caught by {test}");
            }
        }
    }
}

/// Runs one test of this binary in a child with mutant `m` selected; a child that runs longer than 20 s (a mutant
/// that makes a trace loop forever) is killed and counts as failed. Returns `(success, stdout)`.
#[cfg(all(feature = "tr_mutants", not(miri)))]
fn run_child(exe: &std::path::Path, test: &str, m: &str, vb: &str) -> (bool, String) {
    use std::io::Read;
    let out_path = std::env::temp_dir().join(format!(
        "tr_asm_child_{}_{}_{}_{}.txt",
        std::process::id(),
        test,
        m,
        vb
    ));
    let f = std::fs::File::create(&out_path).unwrap();
    let mut child = std::process::Command::new(exe)
        .args(["--exact", test, "--test-threads=1"])
        .env("TR_ASM_CHILD", "1")
        .env("TR_ASM_VB", vb)
        .env("RUSTYNAIL_JIT_MUT", m)
        .stdin(std::process::Stdio::null())
        .stdout(f)
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let t0 = std::time::Instant::now();
    let ok = loop {
        match child.try_wait().unwrap() {
            Some(st) => break st.success(),
            None if t0.elapsed().as_secs() > 20 => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    };
    let mut text = String::new();
    let _ = std::fs::File::open(&out_path).and_then(|mut f| f.read_to_string(&mut text));
    let _ = std::fs::remove_file(&out_path);
    (ok, text)
}

/// The real helper entry points on a real state: `jit_tr_calln` computes through the exit area, `jit_tr_barrier` turns
/// a black table gray again when it is handed a white value (what the trace's slow path asks for).
#[cfg(not(miri))]
#[test]
fn real_helpers_calln_and_barrier_on_a_real_state() {
    use rn_core::gc::{BLACKBIT, WHITEBITS};
    use rn_core::state::LUA_MULTRET;
    let mut st = e2e::new_state(None);
    let base = st.top();
    assert_eq!(
        st.load_buffer(b"return {}, {}", b"=helpers", None),
        rn_core::status::Status::Ok
    );
    let func = st.top() - 1;
    assert_eq!(st.pcall(func, LUA_MULTRET, 0), rn_core::status::Status::Ok);
    let t = st.get(func).as_table().expect("table");
    let v = st.get(func + 1).as_table().expect("table");
    let th = rn_core::jit_hooks::thread_ptr(&mut st);
    // SAFETY: the budget word of the state's per-state data, with the exit area behind it.
    let area = unsafe {
        &mut *rn_jit::rt::trace_budget_ptr(&mut st)
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<ExitArea>()
    };
    // CALLN: pow, fmod, floor
    for (which, x, y, want) in [
        (0u32, 2.0f64, 10.0f64, 1024.0f64),
        (1, 7.5, 2.0, 1.5),
        (2, -2.5, 0.0, -3.0),
    ] {
        area.args[0] = x.to_bits();
        area.args[1] = y.to_bits();
        // SAFETY: `th` is the live thread of `st`; the helper only touches the area.
        unsafe { asm::jit_tr_calln(th.cast(), std::ptr::null_mut(), which, 0) };
        assert_eq!(f64::from_bits(area.args[0]), want, "helper {which}");
    }
    // BARRIER: black table, white value
    let tp = t.as_ptr() as usize;
    let vp = v.as_ptr() as usize;
    // SAFETY: both are live tables of `st`; the marked byte is at the same offset in every object header.
    unsafe {
        assert_ne!(
            (*((vp + table::MARKED) as *const u8)) & WHITEBITS,
            0,
            "a fresh table is white"
        );
        *((tp + table::MARKED) as *mut u8) = BLACKBIT;
    }
    area.args[0] = tp as u64;
    area.args[1] = vp as u64;
    // SAFETY: as above; the table value is `(tag, payload)` of a live table.
    unsafe {
        asm::jit_tr_barrier(
            th.cast(),
            std::ptr::null_mut(),
            u32::from(ctb(LUA_VTABLE)),
            0,
        )
    };
    // SAFETY: live table.
    let after = unsafe { *((tp + table::MARKED) as *const u8) };
    assert_eq!(
        after & BLACKBIT,
        0,
        "the barrier turned the black table gray again"
    );
    st.set_top(base);
}

/// P2-3a (`budget_sub`): in the register form the budget is `sub r, n; jl stub` and the stub refunds `n` before the exit: a failed check leaves the
/// budget word unchanged, a passed one charges `n` (same as the compare-first form). Mutants `budget_refund_skip`, `budget_refund_short`.
#[test]
fn hand_budget_sub_refunds_on_exit() {
    both(|| {
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(4, &[]);
        b.budget(3, s0);
        b.budget(3, s1);
        for sub in [false, true] {
            for cnt in 0..6 {
                let mut w = World::new(&model_ints(&[1]));
                w.load(&model_ints(&[1]));
                w.setup_ci(0);
                w.set_cnt(cnt);
                let (t, _, _, a) = exec(&b.ir, &mut w, |e| e.budget_sub = sub);
                if sub && vb_reg().is_some() {
                    assert!(
                        a.prog
                            .ops
                            .iter()
                            .any(|o| matches!(o, rn_jit::ir::Op::SubJl { .. })),
                        "the fused form is emitted"
                    );
                }
                // cnt < 3: exit 0, word unchanged (refund 0); 3 <= cnt < 6: 3 charged, exit 1, word = cnt - 3 + 4
                let (snap, word) = if cnt < 3 { (0, cnt) } else { (1, cnt - 3 + 4) };
                assert_eq!(snap_of(t), Some(snap), "sub {sub} cnt {cnt}");
                assert_eq!(w.cnt(), word, "sub {sub} cnt {cnt}");
            }
        }
    });
}

/// P2-3b (`poll_off`): the poll reads the word at `[r15 + disp]` (no address in a register): same exits as the register form.
/// Mutant `poll_r15_disp_off`.
#[test]
fn hand_poll_from_r15() {
    both(|| {
        let mut b = B::new();
        let s0 = b.snap(0, &[]);
        let s1 = b.snap(0, &[]);
        b.poll(s0);
        b.budget(5, s1);
        for (irq, want) in [(0u8, 1u32), (1, 0), (255, 0)] {
            let mut w = World::new(&model_ints(&[1]));
            w.load(&model_ints(&[1]));
            w.setup_ci(0);
            w.irq[0] = irq;
            w.set_cnt(0);
            let off = (w.irq.as_ptr() as i64 - w.bud as i64) as i32;
            let irq_addr = w.irq.as_ptr() as u64;
            let (t, _, _, a) = exec(&b.ir, &mut w, |e| e.poll_off = Some(off));
            assert!(
                !a.prog
                    .ops
                    .iter()
                    .any(|o| matches!(o, rn_jit::ir::Op::Mov(7, x) if *x == irq_addr)),
                "no address in a register"
            );
            assert_eq!(snap_of(t), Some(want), "irq {irq}");
        }
    });
}

/// BUNDLE-P P2 `f2ifl`: the guard behind `FFLOOR` is `cvttsd2si; cmp r, 1; jo`. For floats at the edges of the integer range (and a plain value) the guard exits
/// exactly when the floor is NaN / inf / out of range / exactly -2^63 (the one conservative exit), in the emulator and natively, and the `F2iExact` behind other
/// operands still takes the exact test (`2.5` leaves). `f2ifl = false` (the old micro-op) gives the same answers except for the floor of exactly -2^63, which it
/// accepts. Mutants `f2ifl_all` (the cheap guard behind a non-floor operand: `2.5` passes), `f2ifl_never` (the -2^63 floor passes).
#[test]
fn hand_f2ifl_floor_guard() {
    both(|| {
        let edge: &[(f64, bool)] = &[
            (0.5, false),
            (-0.5, false),
            (3.0, false),
            (-0.0, false),
            (2.0f64.powi(62), false),
            (-(2.0f64.powi(62)), false),
            (9223372036854774784.0, false),
            (-9223372036854775808.0, true), // exactly -2^63: the floor is exact, the cheap guard exits conservatively
            (9223372036854775808.0, true),
            (-9223372036854777856.0, true),
            (f64::INFINITY, true),
            (f64::NEG_INFINITY, true),
            (f64::NAN, true),
            (1e300, true),
            (-1e300, true),
        ];
        for floor_first in [true, false] {
            for f2ifl in [true, false] {
                for &(x, conservative) in edge {
                    let mut b = B::new();
                    let s0 = b.snap(0, &[]);
                    let s1 = b.snap(0, &[]);
                    let v = b.sload(0, Ty::Num, s0);
                    let f = if floor_first {
                        b.ins(TOp::Ffloor, Ty::Num, 0, v, NOREF, 0)
                    } else {
                        v
                    };
                    b.ins(TOp::F2iExact, Ty::Int, F_GUARD, f, NOREF, guard_aux(s0, 0));
                    b.budget(1000, s1);
                    let mut m = model_ints(&[1]);
                    m.slots[0] = Val::num(x);
                    let mut w = World::new(&m);
                    w.load(&m);
                    w.setup_ci(0);
                    w.set_cnt(0);
                    let (t, _, _, _) = exec(&b.ir, &mut w, |e| e.f2ifl = f2ifl);
                    let fl = if floor_first { x.floor() } else { x };
                    // exact: the (floored) value is an integer in [-2^63, 2^63)
                    let exact = fl.is_finite()
                        && fl >= -9223372036854775808.0
                        && fl < 9223372036854775808.0
                        && fl == fl.trunc();
                    let cheap = f2ifl && floor_first;
                    let exits = if cheap {
                        !exact || (fl == -9223372036854775808.0)
                    } else {
                        !exact
                    };
                    assert_eq!(
                        snap_of(t),
                        Some(u32::from(!exits)),
                        "x {x:e} floor_first {floor_first} f2ifl {f2ifl}"
                    );
                    if cheap && conservative && fl == -9223372036854775808.0 {
                        assert_eq!(
                            snap_of(t),
                            Some(0),
                            "the floor of -2^63 leaves through the cheap guard"
                        );
                    }
                }
            }
        }
        // a non-integral operand without a floor in front of it must exit (2.5, -2.5, 1e-300) with the knob on
        for x in [2.5, -2.5, 1e-300, 0.1] {
            let mut b = B::new();
            let s0 = b.snap(0, &[]);
            let s1 = b.snap(0, &[]);
            let v = b.sload(0, Ty::Num, s0);
            b.ins(TOp::F2iExact, Ty::Int, F_GUARD, v, NOREF, guard_aux(s0, 0));
            b.budget(1000, s1);
            let mut m = model_ints(&[1]);
            m.slots[0] = Val::num(x);
            let mut w = World::new(&m);
            w.load(&m);
            w.setup_ci(0);
            w.set_cnt(0);
            let (t, _, _, _) = exec(&b.ir, &mut w, |e| e.f2ifl = true);
            assert_eq!(snap_of(t), Some(0), "{x} is not integral: F2I leaves");
        }
    });
}

// ---- N2b: RETF on hand-built IR ------------------------------------------------------------------------------------

/// The world of the RETF tests: the world's `ci` is the returning frame F (rbx = its base; its function slot, the last of the `PRE = 2`
/// slots below, is register `A = 1` of the caller), `caller` is the caller's `CallInfo` (a Lua frame suspended after a `CALL`, its base
/// two slots below F's), and `kc` its constants.
struct RetfWorld {
    w: World,
    caller: Vec<u64>,
    kc: Vec<u64>,
}

const RETF_B: u16 = 2;
const RETF_SITE: u64 = 0x7000_1234;

impl RetfWorld {
    fn new(nres1: u32) -> RetfWorld {
        let m = model_ints(&[10, 20, 30]);
        let mut w = World::new(&m);
        w.load(&m);
        w.setup_ci(0);
        let mut caller = vec![0u64; ci::SIZE.div_ceil(8) + 1];
        let kc = vec![0u64; 8];
        put(&mut caller, ci::CALLSTATUS, 0u32);
        put(&mut caller, ci::SAVEDPC, RETF_SITE as usize);
        put(&mut caller, ci::KCACHE, kc.as_ptr() as usize);
        put(&mut caller, ci::NEXT, w.ci.as_ptr() as usize);
        // the caller's registers 0 and 1 (below the base of F): c0 = int 7, c1 = F's function slot (kept)
        w.frame[0] = u64::from(LUA_VNUMINT);
        w.frame[1] = 7;
        put(&mut w.ci, ci::PREVIOUS, caller.as_ptr() as usize);
        put(&mut w.ci, ci::CALLSTATUS, nres1);
        w.set_cnt(1000);
        RetfWorld { w, caller, kc }
    }
}

/// IR: `RETF` (snapshot 0 = the state before the `RETURN`), then in the caller: an `SLOAD` of its register 0 (the int 7), `+ 1000`, and
/// an always-failing guard whose snapshot (segment 1) writes the sum to the caller's register 2.
fn retf_ir(k_flag: bool, nres1: u32) -> TraceIr {
    let mut b = B::new();
    let s0 = b.snap(0, &[]);
    let site = b.ir.konst(KVal::Ptr(RETF_SITE, PtrKind::Raw));
    let bits = nres1 | if k_flag { RETF_K } else { 0 };
    b.ins(TOp::Retf, Ty::Nil, 0, site, RETF_B, guard_aux(s0, bits));
    // snapshot 1: the caller's segment; its pc (= index, the landing address) is 1
    let s1 = b.snap(0, &[]);
    b.ir.snaps[s1 as usize].seg = 1;
    let c0 = b.sload(0, Ty::Int, s1);
    let k = b.kint(1000);
    let sum = b.ins(TOp::Add, Ty::Int, 0, c0, k, 0);
    let s2 = b.snap(
        0,
        &[SnapEntry {
            slot: 2,
            r: sum,
            flags: 0,
        }],
    );
    b.ir.snaps[s2 as usize].seg = 1;
    let big = b.kint(1 << 40);
    b.ins(TOp::Lt, Ty::Int, 0, big, sum, guard_aux(s2, 0));
    b.ir.segs = vec![
        Seg {
            proto: ProtoKey(1),
            code: 0,
            shift: 0,
            pops: 0,
            call_pc: 0,
            a: 0,
            retf_ins: NOREF,
        },
        Seg {
            proto: ProtoKey(1),
            code: 0,
            shift: RETF_B,
            pops: 1,
            call_pc: 0,
            a: 1,
            retf_ins: NOREF,
        },
    ];
    b.ir
}

struct RetfRun {
    target: u64,
    ci: *mut u8,
    frame: *mut u8,
    k: *const u8,
    th_ci: usize,
}

fn run_retf(rw: &mut RetfWorld, ir: &TraceIr) -> RetfRun {
    run_retf_seed(rw, ir, None)
}

fn run_retf_seed(rw: &mut RetfWorld, ir: &TraceIr, seed: Option<u64>) -> RetfRun {
    let mut env = base_env(&mut rw.w);
    env.stack_off = 0;
    let res = TestRes::linear(ir.snaps.len().max(1));
    let (a, _al, snaps) = compile(ir, &[], seed, &env, &res).unwrap_or_else(|e| panic!("{e}"));
    let log = RefCell::new(Log::default());
    let e = rw.w.env();
    if std::env::var("RETF_DUMP").is_ok() {
        for (n, o) in a.prog.ops.iter().enumerate() {
            eprintln!("{n}: {o:?}");
        }
    }
    let mut hl = Hlp {
        snaps: &snaps,
        stack_size: PRE + NSLOT,
        stack0: rw.w.frame.as_mut_ptr().cast(),
        root: rw.w.ci.as_mut_ptr().cast(),
        code: rw.w.code.as_ptr(),
        log: &log,
    };
    let mut emu = Emu::new(&a.prog, a.entry);
    // SAFETY: the buffers of the world cover every offset the trace uses (frame, ci, thread, area, the caller node).
    let u = unsafe { emu.run(&a.prog, &[], &e, 3_000_000, &mut hl) };
    assert!(matches!(u, Until::Stop(Stop::Unsupported)), "{u:?}");
    if std::env::var("RETF_DUMP").is_ok() {
        eprintln!("regs {:x?}", emu.r);
    }
    RetfRun {
        target: emu.jmp_target().expect("a JmpR target"),
        ci: emu.ci(&e),
        frame: emu.frame(&e),
        k: emu.k(&e),
        th_ci: get::<usize>(&rw.w.thread, thr::CI),
    }
}

#[test]
fn hand_retf_pops_natively_and_exits_in_the_caller() {
    both(|| {
        let ir = retf_ir(false, 2);
        let mut rw = RetfWorld::new(2);
        let rbx = rw.w.rbx() as usize;
        let c_ci = rw.caller.as_ptr() as usize;
        let r = run_retf(&mut rw, &ir);
        // the third guard fails (segment 1): the caller's frame is the running one, the exit stored the sum into its register 2
        assert_eq!(
            snap_of(r.target),
            Some(2),
            "exit through the caller-segment snapshot"
        );
        assert_eq!(r.ci as usize, c_ci, "r13 = the caller");
        assert_eq!(r.th_ci, c_ci, "thread.ci = the caller (retf_no_ci_store)");
        assert_eq!(
            r.frame as usize,
            rbx - 16 * usize::from(RETF_B),
            "rbx moved A + 1 = 2 slots down (retf_shift_off)"
        );
        assert_eq!(
            r.k as usize,
            rw.kc.as_ptr() as usize,
            "r14 = the caller's kcache (retf_no_reloadk)"
        );
        // register 2 of the caller = slot 0 of F's frame: tag int, payload 7 + 1000
        assert_eq!(
            (rw.w.frame[4], rw.w.frame[5]),
            (u64::from(LUA_VNUMINT), 1007),
            "exit store relative to the new base"
        );
    });
}

#[test]
fn hand_retf_guards_exit_before_the_pop() {
    both(|| {
        type Tw = fn(&mut RetfWorld);
        let cases: &[(&str, Tw)] = &[
            // returning frame called from C (`CIST_FRESH`): the return would leave the interpreter loop (retf_callstatus_unchecked)
            ("fresh", |rw| {
                put(&mut rw.w.ci, ci::CALLSTATUS, 2u32 | ci::CIST_FRESH)
            }),
            ("mmret", |rw| {
                put(&mut rw.w.ci, ci::CALLSTATUS, 2u32 | ci::CIST_MMRET)
            }),
            ("hooked", |rw| {
                put(
                    &mut rw.w.ci,
                    ci::CALLSTATUS,
                    2u32 | rn_core::state::CIST_HOOKED,
                )
            }),
            ("nresults", |rw| put(&mut rw.w.ci, ci::CALLSTATUS, 3u32)),
            // the caller is a C frame (retf_cframe_unchecked)
            ("cframe", |rw| {
                put(&mut rw.caller, ci::CALLSTATUS, ci::CIST_C)
            }),
            // the caller is suspended elsewhere (retf_site_unchecked)
            ("site", |rw| {
                put(&mut rw.caller, ci::SAVEDPC, (RETF_SITE + 4) as usize)
            }),
        ];
        for (name, tw) in cases {
            let ir = retf_ir(false, 2);
            let mut rw = RetfWorld::new(2);
            tw(&mut rw);
            let f_ci = rw.w.ci.as_ptr() as usize;
            let rbx = rw.w.rbx() as usize;
            let r = run_retf(&mut rw, &ir);
            assert_eq!(
                snap_of(r.target),
                Some(0),
                "{name}: the guard exits through the RETURN's snapshot"
            );
            assert_eq!(
                (r.ci as usize, r.th_ci, r.frame as usize),
                (f_ci, f_ci, rbx),
                "{name}: nothing popped"
            );
        }
        // bits that are NOT compared: CIST_TAIL (a callee entered by a tail call returns like any other) and CIST_JIT
        for extra in [rn_core::state::CIST_TAIL, 1u32 << 27] {
            let ir = retf_ir(false, 2);
            let mut rw = RetfWorld::new(2 | extra);
            let c_ci = rw.caller.as_ptr() as usize;
            let r = run_retf(&mut rw, &ir);
            assert_eq!(
                (snap_of(r.target), r.ci as usize),
                (Some(2), c_ci),
                "callstatus bit {extra:#x} is not compared"
            );
        }
    });
}

#[test]
fn hand_retf_k_checks_tbc_and_open_upvalues() {
    both(|| {
        // (name, open upvalue address relative to rbx, tbc registered, the pop happens)
        let cases: &[(&str, Option<i64>, bool, bool)] = &[
            ("none", None, false, true),
            ("uv_below", Some(-16), false, true),
            ("uv_at_base", Some(0), false, false),
            ("uv_above", Some(48), false, false),
            ("tbc", None, true, false),
        ];
        for &(name, uv, tbc, pops) in cases {
            let ir = retf_ir(true, 2);
            let mut rw = RetfWorld::new(2);
            let rbx = rw.w.rbx() as i64;
            let mut cell = vec![0u64; 8];
            if let Some(off) = uv {
                put(&mut cell, upval::V, (rbx + off) as usize);
                put(&mut rw.w.thread, thr::OPENUPVAL, cell.as_ptr() as usize);
            }
            if tbc {
                put(&mut rw.w.thread, thr::TBC_LEN, 1usize);
            }
            let f_ci = rw.w.ci.as_ptr() as usize;
            let c_ci = rw.caller.as_ptr() as usize;
            let r = run_retf(&mut rw, &ir);
            let want = if pops {
                (Some(2), c_ci)
            } else {
                (Some(0), f_ci)
            };
            assert_eq!(
                (snap_of(r.target), r.ci as usize),
                want,
                "k-RETF {name} (retf_k_unchecked)"
            );
        }
        // without the k flag the same states do not matter (the RETURN would not close anything)
        let ir = retf_ir(false, 2);
        let mut rw = RetfWorld::new(2);
        put(&mut rw.w.thread, thr::TBC_LEN, 1usize);
        let c_ci = rw.caller.as_ptr() as usize;
        let r = run_retf(&mut rw, &ir);
        assert_eq!((snap_of(r.target), r.ci as usize), (Some(2), c_ci));
    });
}

/// Random register allocation never puts a value in V1 / V2 of a trace with a `RETF` (they are its scratch; a value there would be lost at the
/// exits its guards take), and the trace behaves the same under every allocation.
#[test]
fn hand_retf_random_allocation_keeps_the_scratch_free() {
    both(|| {
        for seed in 0..200u64 {
            let ir = retf_ir(seed % 2 == 0, 2);
            let mut rw = RetfWorld::new(2);
            let c_ci = rw.caller.as_ptr() as usize;
            let r = run_retf_seed(&mut rw, &ir, Some(seed));
            assert_eq!(
                (snap_of(r.target), r.ci as usize),
                (Some(2), c_ci),
                "seed {seed}"
            );
            assert_eq!(
                (rw.w.frame[4], rw.w.frame[5]),
                (u64::from(LUA_VNUMINT), 1007),
                "seed {seed}"
            );
        }
    });
}
