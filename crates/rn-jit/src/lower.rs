//! Bytecode -> micro-op IR: the template set (docs/JIT_DESIGN.md sections 2, 5, 7; arithmetic, comparison,
//! test, upvalue and float-loop templates live in `templates_arith`).
//!
//! Every instruction is either a *template* or an *exit stub*: store the address of the instruction in
//! `ci.savedpc` and leave compiled code with [`JitExit::Continue`]; the interpreter then executes the
//! instruction itself (E3, E4). A template that meets an unusual operand exits at its own instruction
//! *before any side effect*.
//!
//! Block counting (C1): each block head emits one `CountSub{n}` that pre-pays every instruction of the
//! block that is a template (`Plain`, `Guard`, `Branch`; the MMBIN follower of an arithmetic instruction and
//! the JMP follower of a conditional are not executed on the fast path and not counted, exactly like the
//! interpreter). A template that exits at itself refunds the pre-paid count of itself and the rest of its
//! block (`rem`), so an exit never counts an instruction the interpreter will execute again. RETURN0/1 count
//! themselves after their guards instead (kind `Guarded`).

use crate::ir::{Base, Cc, Op, Program, Src, L, M, V, W};
use crate::t15::runtime::Warmup;
use crate::templates_alloc as talloc;
use crate::templates_arith as ta;
use crate::templates_loops as tl;
use crate::templates_table as tt;
use rn_core::instruction::Instruction;
use rn_core::jit_hooks::{thread as thr, JitExit};
use rn_core::jit_layout::callinfo as ci;
use rn_core::value::{Value, LUA_VFALSE, LUA_VNIL, LUA_VTRUE};
use rn_core::ver::active::OpCode;
use std::collections::HashMap;

#[path = "calls.rs"]
pub mod calls;
#[path = "t1inline.rs"]
pub mod t1inline;

/// Largest function (in instructions) that is compiled.
pub const MAX_CODE: usize = 1 << 16;

/// Inputs that depend on where the code lives.
#[derive(Clone, Copy, Debug)]
pub struct LowerCfg<'a> {
    /// Address of `code[0]` (exit stubs embed the address of the instruction to resume at).
    pub code_addr: usize,
    /// Address of the process-wide interrupt flag.
    pub interrupt_addr: usize,
    /// Calls and returns between compiled frames (J1c); `None` keeps the J1a protocol (every RETURN
    /// leaves compiled code, CALL is an exit stub).
    pub calls: Option<calls::CallEnv>,
    /// The constants of the prototype (immutable; templates specialise on their types and values).
    pub consts: &'a [Value],
    /// Tier 1.5 warm-up instrumentation (docs/JIT_TIER15.md 3.3, 6.1): promote counters on the back-edges of the
    /// nests that are still warm and `prof[pc]` tag stores after the loads inside them. `None` = plain tier 1
    /// (the machine code is then byte-identical to what it was before tier 1.5 existed).
    pub t15_warmup: Option<&'a Warmup>,
}

/// The block structure of a lowered function (C1 counting plan): every code version of a prototype must have
/// exactly the same one, because a tier 1.5 exit continues in the tier-1 label of *another* version at a pc in the
/// middle of a block (docs/JIT_TIER15.md section 12, condition 3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockSig {
    /// Is the instruction a block head?
    pub head: Vec<bool>,
    /// Pre-paid instructions at each block head.
    pub cnt_at: Vec<u32>,
    /// Pre-paid instructions from each pc to the end of its block.
    pub rem: Vec<u32>,
    /// Instructions that have a resume entry.
    pub resume_pcs: Vec<u32>,
    /// Does the function have an entry at pc 0 / is it worth entering from the interpreter?
    pub entry_ok: bool,
    /// See [`Lowered::interp_entry`].
    pub interp_entry: bool,
}

/// A pc of a tier 1.5 region as seen by the emulator tests (`Lowered::points`): where tier 1 and region code are in
/// the same state, so that two runs can be compared in lockstep (`tests/t15_core_emu.rs`).
#[derive(Clone, Debug)]
pub struct RegionPoint {
    /// The instruction.
    pub pc: u32,
    /// The tier-1 label of `pc` (before the block count of a head).
    pub t1: L,
    /// The region label of `pc` (before the block count of a head), if region code exists for it. There, the budget
    /// is in `r11` (`V5`) and the slots of `rr` are in their registers (their VM stack slots are stale).
    pub reg: Option<L>,
    /// `(slot, register)` of the integer slots that are register-resident at `reg`.
    pub rr: Vec<(u32, crate::ir::V)>,
    /// `(slot, register)` of the float slots that are register-resident at `reg` (tag `FLT`, bits in the `X`).
    pub rx: Vec<(u32, crate::ir::X)>,
}

/// Result of [`lower`].
#[derive(Debug)]
pub struct Lowered {
    /// Region points of every region of the function (empty without tier 1.5).
    pub points: Vec<RegionPoint>,
    /// The program.
    pub prog: Program,
    /// Resume table: instruction index and the label compiled code is entered at.
    pub resume: Vec<(u32, L)>,
    /// BUNDLE-S S1: the continuations after a table read / store whose Lua metamethod frame tier 1 pushed. Entries only the return of that frame
    /// (`ret_mm`) uses: flagged in the resume table ([`calls::RESUME_MM_ONLY`]), so the interpreter's re-entry points and the trace formation
    /// that depends on them are the same as without the knob. Not part of `resume` (nor of the block signature).
    pub resume_mm: Vec<(u32, L)>,
    /// Number of instructions with a real template (including the MMBIN / JMP followers that the fast path
    /// subsumes).
    pub templates: usize,
    /// Is the first instruction a template? When it is an exit stub, entering at pc 0 would only enter and
    /// leave again (the J1a fib slowdown), so function entries (flat calls, the entry flag) are disabled
    /// for the prototype; loops inside it can still be entered through their resume points (OSR).
    pub entry_ok: bool,
    /// Is it worth entering the function from the *interpreter* (the `TRAP_JIT` flag on a fresh frame)? A small
    /// leaf without loops costs more in the enter/exit round trip than it saves; compiled callers still
    /// reach it with a flat call. (Loops, calls inside, or at least 24 instructions make it worth it.)
    pub interp_entry: bool,
    /// Number of instructions of the function.
    pub total: usize,
    /// Instructions inside loop bodies (union of the `[target, back-edge]` ranges).
    pub loop_total: usize,
    /// ... of which templated.
    pub loop_templates: usize,
    /// The tier-1 label of every instruction compiled code can be continued at (M9: `None` for the *dead* pcs, i.e.
    /// the operand word of a `LOADKX`/`NEWTABLE`..., the `JMP` that a compare consumes and the `MMBIN*` that follows an
    /// arithmetic instruction: continuing there would execute a half instruction).
    pub pc_labels: Vec<Option<L>>,
    /// P-X: the label trace exits land on (`TrProto::pc_addr`): `pc_labels`, except that a mid-block pc with static
    /// register types gets an out-of-line label that verifies the tags first (see `land_checks` in `lower_ex`).
    pub land_labels: Vec<Option<L>>,
    /// Block structure (identical in every version of the same prototype).
    pub sig: BlockSig,
    /// Back-edges `(pc, head)` of the templated loops (informational; tier 1.5 nests are built from them).
    pub backedges: Vec<(u32, u32)>,
}

impl Lowered {
    /// The compile heuristic of `RUSTYNAIL_JIT=on`: compile functions with a loop whose body is at least
    /// 30% templated, and (since calls and returns stay in compiled code, J1c) loop-free functions that have
    /// a function entry and are at least 60% templated, so that calls between them stay compiled.
    pub fn worth_compiling(&self) -> bool {
        (self.loop_total > 0 && self.loop_templates * 100 >= self.loop_total * 30)
            || (self.entry_ok && self.templates * 100 >= self.total * 60)
    }
}

/// What kind of tier-1 landing label an instruction has (the count model of the trace compiler, docs/JIT_TRACE.md 3.7).
#[cfg(feature = "trace")]
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum LabelKind {
    /// No label: the follower of an instruction that subsumes it (`MMBIN*`, the `JMP` of a compare, an operand word).
    /// The interpreter does not fetch it either.
    Dead,
    /// A block head: the label is before the block's `CountSub`, tier 1 holds `budget = B - fetched`.
    Head,
    /// The middle of a block: tier 1 holds `budget = B - fetched - rem(pc)` (the rest of the block was prepaid).
    Mid,
}

/// Count-model API for traces (TR5). Read-only views of the block structure that `lower` computed; every code
/// version of a prototype has the same one (`BlockSig`), so any version may be asked.
#[cfg(feature = "trace")]
impl Lowered {
    /// The landing label kind of `pc` (`Dead` when tier 1 has no label there).
    pub fn label_kind(&self, pc: u32) -> LabelKind {
        let i = pc as usize;
        if self.pc_labels.get(i).copied().flatten().is_none() {
            LabelKind::Dead
        } else if self.sig.head[i] {
            LabelKind::Head
        } else {
            LabelKind::Mid
        }
    }

    /// Instructions prepaid from `pc` to the end of its block (`rem`, 0 for stubs and `Guarded` instructions).
    pub fn rem(&self, pc: u32) -> u32 {
        self.sig.rem[pc as usize]
    }

    /// Instructions prepaid by the block head at `pc` (`cnt_at`; 0 when `pc` is not a head).
    pub fn cnt_at(&self, pc: u32) -> u32 {
        self.sig.cnt_at[pc as usize]
    }

    /// TR5.4 `land(pc)`: what an exit that lands on the tier-1 label of `pc` must subtract: 0 for a head (pre-count
    /// label), `rem(pc)` for a mid-block label. Panics for a `Dead` pc (no exit can land there).
    pub fn land(&self, pc: u32) -> u32 {
        match self.label_kind(pc) {
            LabelKind::Head => 0,
            LabelKind::Mid => self.rem(pc),
            LabelKind::Dead => panic!("no tier-1 label at pc {pc}"),
        }
    }

    /// TR5.1 `fetches`: the number of instructions the interpreter fetches executing the recorded `path` (pcs in
    /// execution order, one per recorded step): the followers tier 1 subsumes (`Dead` pcs) are not fetched.
    pub fn fetches(&self, path: &[u32]) -> u32 {
        path.iter()
            .filter(|&&pc| self.label_kind(pc) != LabelKind::Dead)
            .count() as u32
    }
}

/// How an instruction is compiled.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum Kind {
    /// Template that cannot exit at itself; falls through (or jumps unconditionally).
    Plain,
    /// Template with guards (exits at itself) that falls through. Counted in its block's prepaid count.
    Guard,
    /// Conditional or loop terminator: ends its block, counted in the prepaid count, may exit at itself.
    Branch,
    /// Template with guards that counts itself after them and ends its block (RETURN0/1).
    Guarded,
    /// Exit stub.
    Stub,
}

fn kind(op: Option<OpCode>, i: Instruction, consts: &[Value]) -> Kind {
    match op {
        Some(
            OpCode::Move
            | OpCode::Loadi
            | OpCode::Loadf
            | OpCode::Loadk
            | OpCode::Loadfalse
            | OpCode::Lfalseskip
            | OpCode::Loadtrue
            | OpCode::Loadnil
            | OpCode::Jmp,
        ) => Kind::Plain,
        Some(OpCode::Return0 | OpCode::Return1 | OpCode::Call | OpCode::Tailcall) => Kind::Guarded,
        Some(OpCode::Return) => Kind::Guarded,
        Some(o) => ta::kind(o, i, consts)
            .or_else(|| tt::kind(o, i, consts))
            .or_else(|| tl::kind(o, i))
            .or_else(|| talloc::kind(o, i, consts))
            .unwrap_or(Kind::Stub),
        None => Kind::Stub,
    }
}

/// What follows a template in the bytecode and is subsumed by it.
fn follower(op: OpCode, i: Instruction) -> ta::Follower {
    match op {
        OpCode::Pow | OpCode::Powk => ta::Follower::Mmbin,
        o if talloc::has_extra(o, i) => ta::Follower::Extra,
        o => ta::follower(o),
    }
}

/// Instructions after which control does not fall through to `pc + 1` inside compiled code.
fn ends_block(op: Option<OpCode>) -> bool {
    matches!(op, Some(OpCode::Jmp | OpCode::Lfalseskip))
}

/// Type knowledge about a register inside a block.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Ty {
    /// Nothing known.
    Unk,
    /// Holds an integer.
    Int,
    /// Holds a float.
    Flt,
}

pub(crate) fn fr(r: u32, off: i32) -> M {
    M {
        base: Base::Frame,
        off: (r as i32) * 16 + off,
    }
}
pub(crate) fn ciw(off: usize) -> M {
    M {
        base: Base::Ci,
        off: off as i32,
    }
}
pub(crate) fn thw(off: usize) -> M {
    M {
        base: Base::Thread,
        off: off as i32,
    }
}

pub(crate) struct Cx<'a> {
    pub(crate) p: Program,
    pub(crate) cfg: LowerCfg<'a>,
    pub(crate) pcl: Vec<L>,
    /// The function's bytecode (templates read their JMP followers from it).
    pub(crate) code: &'a [u32],
    /// Exit labels keyed by (pc, refunded pre-paid instructions).
    exit_at: HashMap<(u32, u32), L>,
    exit_trap: HashMap<u32, L>,
    pub(crate) sh: calls::Shared,
    /// Register type knowledge, valid inside the current block (reset at block heads).
    pub(crate) ty: [Ty; 256],
    /// Pre-paid instructions from each pc to the end of its block (inclusive).
    pub(crate) rem: Vec<u32>,
    /// Out-of-line code of the helper templates (`templates_alloc`).
    pub(crate) cold: Vec<talloc::Cold>,
    /// Tier 1.5 (docs/JIT_TIER15.md B3): the label *behind* the `CountSub` of every block head (equal to `pcl` for
    /// heads without a count and in plain tier-1 lowering). Region code that has already paid the block's count
    /// and leaves for tier 1 at the head's own pc jumps here, so the count is not paid twice.
    pub(crate) pcl_post: Vec<L>,
    /// Tier 1.5 (B2): templates that emit out-of-line slow paths record their rejoin label at template time
    /// (`HCall::rejoin`) instead of reading `pcl[next]` when the cold code is emitted. Set only while a region
    /// fallback is being emitted (the tier-1 bytes are unchanged otherwise).
    pub(crate) capture_rejoin: bool,
    /// BUNDLE-B: resume-table entries `(pc + 1, label)` of the comparisons whose metamethod call tier 1 makes: the `JMP` after
    /// the comparison, continued by the label that reads the result the returning metamethod left at `ci.top` (filled by
    /// `emit_cold`, appended to the resume table).
    pub(crate) mm_resume: Vec<(u32, L)>,
    /// BUNDLE-S S1: like `mm_resume`, for the table reads / stores (flagged entries, see [`Lowered::resume_mm`]).
    pub(crate) s1_resume: Vec<(u32, L)>,
    /// BUNDLE-D (`armm`): is the pc a block head? (`Cold::ArithMm`: the block's remaining pre-paid count is paid at the continuation unless the next pc is a head.)
    pub(crate) is_head: Vec<bool>,
    /// BUNDLE-D (`armm`): the pcs after an arithmetic instruction with a metamethod continuation (they get a tag-checking landing label even without traces).
    pub(crate) armm_next: Vec<bool>,
    /// BUNDLE-D: the tag-checking landing label of the mid-block pcs that have one.
    pub(crate) land_of: HashMap<u32, L>,
}

impl Cx<'_> {
    pub(crate) fn pc_addr(&self, pc: u32) -> u64 {
        self.cfg.code_addr as u64 + 4 * u64::from(pc)
    }
    /// Label of "leave with `savedpc` = instruction `pc`" (no refund: nothing of `pc` was counted).
    pub(crate) fn exit_at(&mut self, pc: u32) -> L {
        self.exit_ref(pc, 0)
    }
    /// The exit of a failed `CountSub{n}` at `pc` (block head, or a one-instruction count): `savedpc = pc`, and the
    /// `n` just subtracted is given back, so the counter is the exact budget left and `rt::enter` counts only the
    /// instructions that really ran (the interpreter executes the block again). P0y / G1.
    pub(crate) fn exit_cnt(&mut self, pc: u32, n: u32) -> L {
        self.exit_ref(pc, n)
    }
    /// Like `exit_at` for a template at `pc` whose count was pre-paid: refunds `rem[pc]`.
    pub(crate) fn exit_rx(&mut self, pc: u32, extra: u32) -> L {
        let r = self.rem[pc as usize] + extra;
        self.exit_ref(pc, r)
    }
    /// Like `exit_cnt` with an explicit refund.
    pub(crate) fn exit_ref_pub(&mut self, pc: u32, refund: u32) -> L {
        self.exit_ref(pc, refund)
    }
    pub(crate) fn exit_r(&mut self, pc: u32) -> L {
        let r = self.rem[pc as usize];
        self.exit_ref(pc, r)
    }
    /// Copies of the lazily created exit sequences (tier 1.5 takes a refused region back).
    pub(crate) fn exit_at_snapshot(&self) -> (HashMap<(u32, u32), L>, HashMap<u32, L>) {
        (self.exit_at.clone(), self.exit_trap.clone())
    }
    pub(crate) fn exit_restore(&mut self, s: (HashMap<(u32, u32), L>, HashMap<u32, L>)) {
        (self.exit_at, self.exit_trap) = s;
    }
    fn exit_ref(&mut self, pc: u32, refund: u32) -> L {
        if let Some(&l) = self.exit_at.get(&(pc, refund)) {
            return l;
        }
        let l = self.p.label();
        self.exit_at.insert((pc, refund), l);
        l
    }
    /// Like `exit_at` but also sets `ci.trap` so the interpreter re-examines hooks/interrupts.
    pub(crate) fn exit_trap(&mut self, pc: u32) -> L {
        if let Some(&l) = self.exit_trap.get(&pc) {
            return l;
        }
        let l = self.p.label();
        self.exit_trap.insert(pc, l);
        l
    }
    pub(crate) fn op(&mut self, o: Op) {
        self.p.push(o);
    }
    pub(crate) fn st_imm(&mut self, w: W, m: M, v: i32) {
        self.op(Op::St {
            w,
            m,
            src: Src::Imm(v),
        });
    }
    pub(crate) fn st(&mut self, w: W, m: M, v: V) {
        self.op(Op::St {
            w,
            m,
            src: Src::V(v),
        });
    }
    pub(crate) fn ld(&mut self, dst: V, w: W, m: M) {
        self.op(Op::Ld { dst, w, m });
    }
    pub(crate) fn br(&mut self, cc: Cc, a: V, b: Src, to: L) {
        self.op(Op::Br { cc, a, b, to });
    }
    /// Copy a whole value (tag byte + payload) from `src` to `dst` (clobbers V5, V6).
    pub(crate) fn copy_value(&mut self, dst: M, src: M) {
        let (dt, dp) = (
            dst,
            M {
                off: dst.off + 8,
                ..dst
            },
        );
        let (st, sp) = (
            src,
            M {
                off: src.off + 8,
                ..src
            },
        );
        self.ld(5, W::B1, st);
        self.ld(6, W::B8, sp);
        self.st(W::B1, dt, 5);
        self.st(W::B8, dp, 6);
    }
    pub(crate) fn set_const(&mut self, r: u32, tag: u8, payload: i32) {
        self.st_imm(W::B1, fr(r, 0), i32::from(tag));
        self.st_imm(W::B8, fr(r, 8), payload);
        self.ty[r as usize] = Ty::Unk;
    }
    /// Back-edge poll: pending `ci.trap` or interrupt flag -> leave at `target`.
    pub(crate) fn poll(&mut self, target: u32) {
        let x = self.exit_trap(target);
        self.ld(0, W::B4, ciw(ci::TRAP));
        self.br(Cc::Ne, 0, Src::Imm(0), x);
        self.op(Op::Mov(1, self.cfg.interrupt_addr as u64));
        self.ld(
            2,
            W::B1,
            M {
                base: Base::Reg(1),
                off: 0,
            },
        );
        self.br(Cc::Ne, 2, Src::Imm(0), x);
    }
    /// `if ci.callstatus & CIST_FRESH` -> `Returned`, else `FrameChanged`.
    fn finish_return(&mut self) {
        if self.calls_on() {
            self.jmp_ret_tail();
            return;
        }
        let fresh = self.p.label();
        self.ld(0, W::B4, ciw(ci::CALLSTATUS));
        self.op(Op::And(0, Src::Imm(ci::CIST_FRESH as i32)));
        self.br(Cc::Ne, 0, Src::Imm(0), fresh);
        self.op(Op::Ret(JitExit::FrameChanged as u32));
        self.op(Op::Bind(fresh));
        self.op(Op::Ret(JitExit::Returned as u32));
    }
    /// Common prefix of RETURN0/RETURN1: guards, block count, pops the frame (`st.ci = prev`) and leaves
    /// `V0 = nresults` (-1 = multret), `V3 = func index`.
    fn return_prologue(&mut self, pc: u32) {
        let x = self.exit_at(pc);
        self.ld(0, W::B4, ciw(ci::TRAP));
        self.br(Cc::Ne, 0, Src::Imm(0), x); // hooks: the interpreter handles the hook protocol
        self.ld(0, W::B4, ciw(ci::CALLSTATUS));
        self.op(Op::MovR(1, 0));
        self.op(Op::And(1, Src::Imm(ci::CIST_MMRET as i32)));
        // metamethod frame: `mm_return` is the interpreter's, except that the frame of a comparison that tier 1 started
        // (`Cold::MmRet`) finishes it here
        let xm = self.mm_ret_label(pc, x);
        self.br(Cc::Ne, 1, Src::Imm(0), xm);
        self.ld(2, W::B8, ciw(ci::PREVIOUS));
        self.br(Cc::Eq, 2, Src::Imm(0), x);
        if self.calls_on() {
            self.return_poll(pc);
        }
        let xc = self.exit_cnt(pc, 1);
        self.op(Op::CountSub { n: 1, exit: xc });
        self.op(Op::And(0, Src::Imm(ci::NRESULTS_MASK as i32)));
        self.op(Op::Sub(0, Src::Imm(1)));
        self.ld(3, W::B8, ciw(ci::FUNC));
        self.st(W::B8, thw(thr::CI), 2);
    }

    pub(crate) fn template(&mut self, pc: u32, i: Instruction, op: OpCode) {
        let a = i.a();
        match op {
            OpCode::Move => {
                self.copy_value(fr(a, 0), fr(i.b(), 0));
                self.ty[a as usize] = self.ty[i.b() as usize];
            }
            OpCode::Loadi => {
                self.set_const(a, rn_core::value::LUA_VNUMINT, i.sbx());
                self.ty[a as usize] = Ty::Int;
            }
            OpCode::Loadf => {
                self.op(Op::Mov(0, f64::from(i.sbx()).to_bits()));
                self.st_imm(W::B1, fr(a, 0), i32::from(rn_core::value::LUA_VNUMFLT));
                self.st(W::B8, fr(a, 8), 0);
                self.ty[a as usize] = Ty::Flt;
            }
            OpCode::Loadk => self.load_k(a, i.bx()),
            OpCode::Loadfalse => self.set_const(a, LUA_VFALSE, 0),
            OpCode::Lfalseskip => {
                self.set_const(a, LUA_VFALSE, 0);
                let t = self.pcl[pc as usize + 2];
                self.op(Op::Jmp(t));
            }
            OpCode::Loadtrue => self.set_const(a, LUA_VTRUE, 0),
            OpCode::Loadnil => {
                for n in 0..=i.b() {
                    self.set_const(a + n, LUA_VNIL, 0);
                }
            }
            OpCode::Jmp => {
                let t = (pc as i64 + 1 + i64::from(i.sj())) as u32;
                if i.sj() < 0 {
                    self.poll(t);
                }
                let l = self.pcl[t as usize];
                self.op(Op::Jmp(l));
            }
            OpCode::Call => self.tpl_call(pc, i),
            OpCode::Tailcall => self.tpl_tailcall(pc, i),
            OpCode::Tforprep
            | OpCode::Tforcall
            | OpCode::Tforloop
            | OpCode::Varargprep
            | OpCode::Vararg
            | OpCode::Close
            | OpCode::Tbc => self.t_loops(pc, i, op),
            OpCode::Return => self.tpl_return(pc, i),
            OpCode::Return0 => {
                self.return_prologue(pc);
                let (lloop, ldone) = (self.p.label(), self.p.label());
                self.op(Op::Lea(4, fr(0, -16)));
                self.op(Op::Bind(lloop));
                self.br(Cc::Le, 0, Src::Imm(0), ldone);
                self.nil_at_v4();
                self.op(Op::Add(3, Src::Imm(1)));
                self.op(Op::Sub(0, Src::Imm(1)));
                self.op(Op::Jmp(lloop));
                self.op(Op::Bind(ldone));
                self.st(W::B8, thw(thr::TOP), 3);
                self.finish_return();
            }
            OpCode::Return1 => {
                self.return_prologue(pc);
                let (lsome, lend, lloop, lexit) = (
                    self.p.label(),
                    self.p.label(),
                    self.p.label(),
                    self.p.label(),
                );
                self.br(Cc::Ne, 0, Src::Imm(0), lsome);
                self.st(W::B8, thw(thr::TOP), 3); // no results wanted: top = func
                self.op(Op::Jmp(lexit));
                self.op(Op::Bind(lsome));
                self.copy_value(fr(0, -16), fr(a, 0));
                self.op(Op::Add(3, Src::Imm(1)));
                self.op(Op::Lea(4, fr(0, 0)));
                self.op(Op::Bind(lloop));
                self.br(Cc::Le, 0, Src::Imm(1), lend);
                self.nil_at_v4();
                self.op(Op::Add(3, Src::Imm(1)));
                self.op(Op::Sub(0, Src::Imm(1)));
                self.op(Op::Jmp(lloop));
                self.op(Op::Bind(lend));
                self.st(W::B8, thw(thr::TOP), 3);
                self.op(Op::Bind(lexit));
                self.finish_return();
            }
            OpCode::Geti
            | OpCode::Gettable
            | OpCode::Getfield
            | OpCode::Gettabup
            | OpCode::SelfOp
            | OpCode::Seti
            | OpCode::Settable
            | OpCode::Setfield
            | OpCode::Settabup => self.t_table(pc, i, op),
            OpCode::Newtable
            | OpCode::Setlist
            | OpCode::Len
            | OpCode::Concat
            | OpCode::Closure
            | OpCode::Pow
            | OpCode::Powk => self.t_alloc(pc, i, op),
            _ => self.t_arith(pc, i, op),
        }
    }

    /// `*V4 = nil; V4 += 16`.
    fn nil_at_v4(&mut self) {
        let m = M {
            base: Base::Reg(4),
            off: 0,
        };
        self.st_imm(W::B1, m, i32::from(LUA_VNIL));
        self.st_imm(W::B8, M { off: 8, ..m }, 0);
        self.op(Op::Add(4, Src::Imm(16)));
    }
}

/// Removes `Jmp l` directly followed by `Bind(l)` (possibly through other binds) and code that follows an
/// unconditional jump or return up to the next label.
fn peephole(ops: Vec<Op>) -> Vec<Op> {
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    let mut dead = false;
    for (idx, op) in ops.iter().enumerate() {
        if matches!(op, Op::Bind(_)) {
            dead = false;
        } else if dead {
            continue;
        }
        if let Op::Jmp(l) = op {
            let mut j = idx + 1;
            let mut falls = false;
            while let Some(Op::Bind(b)) = ops.get(j) {
                if b == l {
                    falls = true;
                    break;
                }
                j += 1;
            }
            dead = true;
            if falls {
                continue;
            }
        }
        if matches!(op, Op::Ret(_)) {
            dead = true;
        }
        out.push(*op);
    }
    out
}

/// Loads whose result tag the warm-up code records in `prof[pc]` (docs/JIT_TIER15.md 3.3 item 3).
fn is_prof_load(op: OpCode) -> bool {
    matches!(
        op,
        OpCode::Gettable
            | OpCode::Geti
            | OpCode::Getfield
            | OpCode::Gettabup
            | OpCode::Getupval
            | OpCode::SelfOp
    )
}

/// Redirects the branch targets of `op` that equal `from` to `to`.
fn retarget(op: &mut Op, from: L, to: L) {
    match op {
        Op::Jmp(l) if *l == from => *l = to,
        Op::Br { to: l, .. }
        | Op::FBr { to: l, .. }
        | Op::FBrX { to: l, .. }
        | Op::BrM { to: l, .. }
        | Op::BrMi { to: l, .. }
            if *l == from =>
        {
            *l = to;
        }
        _ => {}
    }
}

/// [`retarget`] for several labels at once: `stub_of[l - base]` is the new target of label `l` (`NO_STUB`: none).
fn retarget_any(op: &mut Op, base: L, stub_of: &[L]) {
    let l = match op {
        Op::Jmp(l) => l,
        Op::Br { to: l, .. }
        | Op::FBr { to: l, .. }
        | Op::FBrX { to: l, .. }
        | Op::BrM { to: l, .. }
        | Op::BrMi { to: l, .. } => l,
        _ => return,
    };
    if let Some(&to) = l.checked_sub(base).and_then(|d| stub_of.get(d as usize)) {
        if to != NO_STUB {
            *l = to;
        }
    }
}

/// "No stub" in the table of [`retarget_any`].
const NO_STUB: L = L::MAX;

impl Cx<'_> {
    /// Out-of-line promote counter of the back-edges `(nest ni -> head)`: decrement the nest's counter; when it
    /// goes negative call `jit_t15_promote`, which returns 0 ("go on in this code") or 1 (the continuation address
    /// for `head` in the new code is in the nest-independent cell `cont_addr`). Registers: tier-1 code keeps
    /// nothing in virtual registers between instructions, the frame is exact (everything lives in slots).
    fn emit_warm_stub(&mut self, w: &Warmup, ni: usize, head: u32, l: L) {
        let (fire, stay) = (self.p.label(), self.p.label());
        let ctr = M {
            base: Base::Reg(0),
            off: 0,
        };
        self.op(Op::Bind(l));
        self.op(Op::Mov(0, w.nests[ni].ctr_addr as u64));
        self.ld(1, W::B4, ctr);
        self.op(Op::Sub(1, Src::Imm(1)));
        self.st(W::B4, ctr, 1);
        self.br(Cc::Lt, 1, Src::Imm(0), fire);
        let hl = self.pcl[head as usize];
        self.op(Op::Jmp(hl));
        self.op(Op::Bind(fire));
        self.op(Op::CallH {
            f: w.promote_fn,
            a: w.nests[ni].id | (u32::from(w.ver) << 16),
            b: head,
        });
        if let Some(env) = self.cfg.calls {
            // (the helper moves nothing, but the frame base is cheap to re-derive on this cold path)
            self.op(Op::ReloadFrame {
                stack_off: env.stack_off,
            });
        }
        self.br(Cc::Eq, 0, Src::Imm(0), stay);
        self.op(Op::Mov(1, w.cont_addr as u64));
        self.ld(
            1,
            W::B8,
            M {
                base: Base::Reg(1),
                off: 0,
            },
        );
        self.op(Op::JmpR(1));
        self.op(Op::Bind(stay));
        self.op(Op::Jmp(hl));
    }
}

/// What the tier 1.5 region generator needs to know about the tier-1 lowering (docs/JIT_TIER15.md section 16).
pub(crate) struct RegionEnv<'a> {
    /// Decoded instructions.
    pub ins: &'a [Instruction],
    /// Opcodes.
    pub ops: &'a [Option<OpCode>],
    /// How each pc is compiled (`Stub` = exit stub).
    pub kinds: &'a [Kind],
    /// Tier-1 block heads (length `n`).
    pub head: &'a [bool],
    /// Pre-paid instructions at each head.
    pub cnt_at: &'a [u32],
    /// Instructions with a resume entry (length `n`).
    pub resume: &'a [bool],
    /// Subsumed followers: MMBIN* (`cold`) and JMP / EXTRAARG (`dead`).
    pub follower: &'a [bool],
    /// `+verify` only (empty otherwise): the type knowledge `Cx::ty` had when tier 1 emitted the code of each pc
    /// (`(slot, ty)` for the slots that are not `Unk`).
    pub ty_at: &'a [Vec<(u8, Ty)>],
    /// Back-edges `(pc, head)` of the templated loops (J5: the trace dispatch hook routes every edge into these heads).
    #[cfg_attr(not(feature = "trace"), allow(dead_code))]
    pub backedges: &'a [(u32, u32)],
    /// J5/F5 (`Warmup::via_dispatch`): loop head -> label of J4's promote-counter stub `W_H` (TR19b(b)); empty otherwise.
    #[cfg_attr(not(feature = "trace"), allow(dead_code))]
    pub warm_stub: &'a HashMap<u32, L>,
}

/// What a region hook hands back to [`lower_ex`].
#[derive(Default)]
pub(crate) struct RegionOut {
    /// For tests: see [`RegionPoint`].
    pub points: Vec<RegionPoint>,
    /// `(loop head pc, label of its entry stub)`: every jump of tier-1 code to the head's tier-1 label (entries and
    /// back-edges of the tier-1 copy) and the fall-through into it are redirected to the stub.
    pub heads: Vec<(u32, L)>,
    /// `(pc, stub label)`: the resume-table entry and the continuation label of `pc` become the stub.
    pub overrides: Vec<(u32, L)>,
    /// The stubs emitted with failure accounting (tier 1.5 policy, docs/JIT_TIER15.md 21).
    pub sites: Vec<crate::t15::runtime::StubSite>,
    /// `(pc, stub label)`: like `overrides`, but only the resume-table entry of `pc` becomes the stub; the tier-1
    /// continuation label (`pc_labels`) stays the real one (J5 trace dispatch: exits land *behind* the dispatch jump).
    pub resume_overrides: Vec<(u32, L)>,
}

/// The hook type of [`lower_ex`]: called after the tier-1 code of every instruction was emitted and before the
/// out-of-line code, with the (still open) `Cx`.
pub(crate) type RegionHook<'h> = dyn FnMut(&mut Cx<'_>, &RegionEnv<'_>) -> RegionOut + 'h;

/// Lowers `code` (a whole function). `None` = refuse to compile (empty or oversized code,
/// jump out of range, code that can fall off its end).
pub fn lower(code: &[u32], cfg: LowerCfg<'_>) -> Option<Lowered> {
    lower_ex(code, cfg, None, false)
}

/// [`lower`] plus an optional tier 1.5 region hook.
pub(crate) fn lower_ex(
    code: &[u32],
    cfg: LowerCfg<'_>,
    mut hook: Option<&mut RegionHook<'_>>,
    record_ty: bool,
) -> Option<Lowered> {
    let n = code.len();
    if n == 0 || n > MAX_CODE {
        return None;
    }
    let ins: Vec<Instruction> = code.iter().map(|&w| Instruction(w)).collect();
    let ops: Vec<Option<OpCode>> = ins.iter().map(|i| i.opcode()).collect();
    let mut kinds: Vec<Kind> = (0..n)
        .map(|pc| kind(ops[pc], ins[pc], cfg.consts))
        .collect();
    // A fast-path template subsumes its follower: the MMBIN* after an arithmetic instruction (moved out of
    // line, never executed on the fast path) or the JMP after a conditional (the template jumps itself).
    // Without the expected follower the instruction stays an exit stub.
    if cfg.calls.is_none() {
        for k in kinds.iter_mut().zip(&ops).filter_map(|(k, o)| {
            (matches!(
                o,
                Some(
                    OpCode::Call
                        | OpCode::Tailcall
                        | OpCode::Return
                        | OpCode::Newtable
                        | OpCode::Setlist
                        | OpCode::Len
                        | OpCode::Concat
                        | OpCode::Closure
                        | OpCode::Pow
                        | OpCode::Powk
                )
            ) || tl::needs_calls(*o))
            .then_some(k)
        }) {
            *k = Kind::Stub;
        }
    }
    let mut cold = vec![false; n];
    let mut dead = vec![false; n];
    for pc in 0..n {
        let want = match kinds[pc] {
            Kind::Guard | Kind::Branch => {
                follower(ops[pc].expect("template has an opcode"), ins[pc])
            }
            _ => ta::Follower::None,
        };
        let ok = |f: fn(Option<OpCode>) -> bool| pc + 2 < n && f(ops[pc + 1]);
        match want {
            ta::Follower::None => {}
            ta::Follower::Mmbin => {
                if ok(|o| matches!(o, Some(OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink))) {
                    cold[pc + 1] = true;
                } else {
                    kinds[pc] = Kind::Stub;
                }
            }
            ta::Follower::Extra => {
                if ok(|o| matches!(o, Some(OpCode::Extraarg))) {
                    dead[pc + 1] = true;
                } else {
                    kinds[pc] = Kind::Stub;
                }
            }
            ta::Follower::Jmp => {
                if ok(|o| matches!(o, Some(OpCode::Jmp))) {
                    dead[pc + 1] = true;
                } else {
                    kinds[pc] = Kind::Stub;
                }
            }
        }
    }
    let mut head = vec![false; n + 1];
    let mut resume = vec![false; n + 1];
    head[0] = true;
    resume[0] = true;
    let in_range = |t: i64| (0..n as i64).contains(&t);
    for pc in 0..n {
        if cold[pc] || dead[pc] {
            continue;
        }
        let i = ins[pc];
        let p = pc as i64;
        match (ops[pc], kinds[pc]) {
            (Some(OpCode::Jmp), Kind::Plain) => {
                let t = p + 1 + i64::from(i.sj());
                if !in_range(t) {
                    return None;
                }
                head[t as usize] = true;
                resume[t as usize] |= i.sj() < 0;
                head[pc + 1] = true;
            }
            (Some(OpCode::Forloop), Kind::Branch) => {
                let t = p + 1 - i64::from(i.bx());
                if !in_range(t) || pc + 1 >= n {
                    return None;
                }
                head[t as usize] = true;
                resume[t as usize] = true;
                head[pc + 1] = true;
            }
            (Some(OpCode::Forprep), Kind::Branch) => {
                let t = p + i64::from(i.bx()) + 2;
                if !in_range(t) || pc + 1 >= n {
                    return None;
                }
                head[t as usize] = true;
                head[pc + 1] = true;
            }
            (Some(OpCode::Lfalseskip), _) => {
                if pc + 2 >= n {
                    return None;
                }
                head[pc + 2] = true;
                head[pc + 1] = true;
            }
            (Some(OpCode::Call), Kind::Guarded) => {
                // the one re-entry point after an instruction: a compiled callee returning here
                // (only when the next instruction is a template: jumping into a stub would just leave again,
                // and every bounce through the entry thunk costs more than the interpreter's return)
                head[pc + 1] = true;
                resume[pc + 1] = pc + 1 < n && kinds[pc + 1] != Kind::Stub;
            }
            (Some(OpCode::Return0 | OpCode::Return1 | OpCode::Return | OpCode::Tailcall), _) => {
                head[pc + 1] = true;
            }
            (Some(OpCode::Tforprep), Kind::Guard) => {
                // jumps to its TFORCALL (the end of the loop); the body follows
                let t = p + 1 + i64::from(i.bx());
                if !in_range(t) || pc + 1 >= n {
                    return None;
                }
                head[t as usize] = true;
                head[pc + 1] = true;
            }
            (
                Some(
                    OpCode::Tforcall
                    | OpCode::Varargprep
                    | OpCode::Vararg
                    | OpCode::Close
                    | OpCode::Tbc,
                ),
                Kind::Guarded,
            ) => {
                head[pc + 1] = true;
            }
            (Some(OpCode::Tforloop), _) => {
                // a back-edge (loop tick target) like FORLOOP, executed by the interpreter
                let t = p + 1 - i64::from(i.bx());
                if in_range(t) {
                    head[t as usize] = true;
                    resume[t as usize] = true;
                }
                head[pc + 1] = true;
            }
            (_, Kind::Stub) => {
                // not a re-entry point: the instruction after an exit stub may be an MMBIN/EXTRAARG/JMP
                head[pc + 1] = true;
            }
            (_, Kind::Branch) => {
                // conditional: the jump of its JMP follower, or the instruction after the follower
                let j = ins[pc + 1];
                let t = p + 2 + i64::from(j.sj());
                if !in_range(t) {
                    return None;
                }
                head[t as usize] = true;
                resume[t as usize] |= j.sj() < 0;
                head[pc + 2] = true;
            }
            _ => {}
        }
    }
    if (0..n).any(|t| head[t] && (cold[t] || dead[t])) {
        return None; // a jump into a subsumed follower: not code the compiler produces
    }
    // the last instruction must not fall off the end
    let last = ops[n - 1];
    if kinds[n - 1] != Kind::Stub
        && !matches!(
            last,
            Some(OpCode::Return0 | OpCode::Return1 | OpCode::Return | OpCode::Jmp)
        )
    {
        return None;
    }
    if dead[n - 1] || cold[n - 1] {
        return None;
    }

    // Block counting plan: pre-paid instruction count per head, refund per instruction.
    let mut cnt_at = vec![0u32; n];
    let mut rem = vec![0u32; n];
    for pc in 0..n {
        if !head[pc] {
            continue;
        }
        let mut list: Vec<usize> = Vec::new();
        let mut q = pc;
        while q < n {
            match kinds[q] {
                Kind::Stub | Kind::Guarded => break,
                Kind::Plain => {
                    list.push(q);
                    if ends_block(ops[q]) {
                        break;
                    }
                }
                Kind::Guard => list.push(q),
                Kind::Branch => {
                    list.push(q);
                    break;
                }
            }
            q += if q + 1 < n && (cold[q + 1] || dead[q + 1]) {
                2
            } else {
                1
            };
            if q >= n || head[q] {
                break;
            }
        }
        cnt_at[pc] = list.len() as u32;
        for (idx, &qq) in list.iter().enumerate() {
            rem[qq] = (list.len() - idx) as u32;
        }
    }

    // loop bodies, for the compile heuristic
    let mut inloop = vec![false; n];
    let mut backedges: Vec<(u32, u32)> = Vec::new();
    for pc in 0..n {
        if cold[pc] || dead[pc] {
            continue;
        }
        let i = ins[pc];
        let p = pc as i64;
        let back = match (ops[pc], kinds[pc]) {
            (Some(OpCode::Forloop), Kind::Branch) => Some(p + 1 - i64::from(i.bx())),
            (Some(OpCode::Jmp), Kind::Plain) if i.sj() < 0 => Some(p + 1 + i64::from(i.sj())),
            (Some(OpCode::Tforloop), _) => Some(p + 1 - i64::from(i.bx())),
            (_, Kind::Branch) if ops[pc] != Some(OpCode::Forprep) && pc + 1 < n => {
                let j = ins[pc + 1];
                (j.sj() < 0).then_some(p + 2 + i64::from(j.sj()))
            }
            _ => None,
        };
        if let Some(t) = back {
            if in_range(t) && t <= p {
                backedges.push((pc as u32, t as u32));
                for x in &mut inloop[t as usize..=pc] {
                    *x = true;
                }
            }
        }
    }
    let loop_total = inloop.iter().filter(|&&b| b).count();
    let loop_templates = (0..n)
        .filter(|&pc| inloop[pc] && (kinds[pc] != Kind::Stub || cold[pc] || dead[pc]))
        .count();

    let warm = cfg.t15_warmup;
    let be_head: HashMap<u32, u32> = if warm.is_some() {
        backedges.iter().copied().collect()
    } else {
        HashMap::new()
    };
    let mut warm_lbl: HashMap<(usize, u32), L> = HashMap::new();
    let mut warm_by_head: HashMap<u32, L> = HashMap::new();
    // (J5/F5) back-edges whose promote counter is reached through the dispatch cell: if the hook does not dispatch the
    // head after all, the edge goes through the counter directly, as without traces
    let mut warm_deferred: Vec<(std::ops::Range<usize>, L, L, u32)> = Vec::new();
    let mut resume_alias: Vec<(usize, L)> = Vec::new();
    // BUNDLE-D (`armm`): the pcs after the arithmetic instructions that continue in compiled code after their metamethod returned
    let mut armm_next = vec![false; n + 2];
    if cfg.calls.is_some_and(|e| e.armm) {
        for pc in 0..n {
            if kinds[pc] == Kind::Stub || cold[pc] || dead[pc] {
                continue;
            }
            match ops[pc] {
                Some(OpCode::Unm | OpCode::Bnot) => armm_next[pc + 1] = true,
                Some(o) if ta::follower(o) == ta::Follower::Mmbin && pc + 2 < n && cold[pc + 1] => {
                    armm_next[pc + 2] = true
                }
                _ => {}
            }
        }
    }
    // BUNDLE-S S1 (`t1q` bits 2 / 3): the pcs after the table reads / stores that continue in compiled code after a Lua `__index` / `__newindex` frame returned
    if cfg.calls.is_some_and(|e| e.t1q & 12 != 0) {
        for pc in 0..n {
            if kinds[pc] == Kind::Stub || cold[pc] || dead[pc] {
                continue;
            }
            if matches!(
                ops[pc],
                Some(
                    OpCode::Gettabup
                        | OpCode::Gettable
                        | OpCode::Geti
                        | OpCode::Getfield
                        | OpCode::SelfOp
                        | OpCode::Settabup
                        | OpCode::Settable
                        | OpCode::Seti
                        | OpCode::Setfield
                )
            ) {
                armm_next[pc + 1] = true;
            }
        }
    }
    let mut cx = Cx {
        p: Program::default(),
        cfg,
        pcl: Vec::new(),
        code,
        exit_at: HashMap::new(),
        exit_trap: HashMap::new(),
        sh: calls::Shared::default(),
        ty: [Ty::Unk; 256],
        rem,
        cold: Vec::new(),
        pcl_post: Vec::new(),
        capture_rejoin: false,
        mm_resume: Vec::new(),
        s1_resume: Vec::new(),
        is_head: head[..n].to_vec(),
        armm_next,
        land_of: HashMap::new(),
    };
    cx.pcl = (0..n).map(|_| cx.p.label()).collect();
    cx.pcl_post = cx.pcl.clone();
    let mut bind_idx: Vec<usize> = vec![usize::MAX; n];
    let mut placeholder: Vec<bool> = vec![false; n];
    let mut be_target: Vec<bool> = vec![false; n];
    for &(_, t) in &backedges {
        be_target[t as usize] = true;
    }
    let mut ty_at: Vec<Vec<(u8, Ty)>> = if record_ty && hook.is_some() {
        vec![Vec::new(); n]
    } else {
        Vec::new()
    };
    let mut templates = 0;
    let mut cold_list: Vec<usize> = Vec::new();
    // P-X: `(pc, landing label, registers with a static type at pc)` of the mid-block labels with type knowledge
    let mut land_checks: Vec<(usize, L, Vec<(u32, Ty)>)> = Vec::new();
    for pc in 0..n {
        if dead[pc] {
            templates += 1;
            continue;
        }
        if cold[pc] {
            templates += 1;
            cold_list.push(pc);
            continue;
        }
        let (i, o) = (ins[pc], ops[pc]);
        let k = kinds[pc];
        if hook.is_some() && (pc == 0 || be_target[pc]) {
            // P-G: a placeholder the hook turns into `Jmp(stub)` in place (the fall-through into a dispatched head goes
            // through its stub) instead of inserting into the op vector, which moved every op once per compile
            let ph = cx.p.label();
            cx.op(Op::Bind(ph));
            placeholder[pc] = true;
        }
        bind_idx[pc] = cx.p.ops.len();
        cx.op(Op::Bind(cx.pcl[pc]));
        if k == Kind::Stub {
            cx.exit_at.insert((pc as u32, 0), cx.pcl[pc]);
            let addr = cx.pc_addr(pc as u32);
            cx.op(Op::Mov(0, addr));
            cx.st(W::B8, ciw(ci::SAVEDPC), 0);
            cx.op(Op::Ret(JitExit::Continue as u32));
            continue;
        }
        if head[pc] {
            cx.ty = [Ty::Unk; 256];
            if cnt_at[pc] > 0 {
                let x = cx.exit_cnt(pc as u32, cnt_at[pc]);
                cx.op(Op::CountSub {
                    n: cnt_at[pc],
                    exit: x,
                });
                if hook.is_some() {
                    let post = cx.p.label();
                    cx.op(Op::Bind(post));
                    cx.pcl_post[pc] = post;
                }
            }
        }
        // P-X: a mid-block label inherits the static types of the registers from the straight-line code before it
        // (`cx.ty`; a template with a partial leaf such as MODK / IDIV / the bitwise operators leaves its result typed
        // Int because the float case exits to the interpreter). A trace exit that lands on such a label can carry another
        // type (the trace handles the float case itself), so the label the exits land on checks the tags first (below).
        if (cfg!(feature = "trace") || cx.armm_next[pc]) && !head[pc] {
            let known: Vec<(u32, Ty)> = (0..=255u32)
                .filter(|&s| cx.ty[s as usize] != Ty::Unk)
                .map(|s| (s, cx.ty[s as usize]))
                .collect();
            if !known.is_empty() {
                let l = cx.p.label();
                cx.land_of.insert(pc as u32, l);
                land_checks.push((pc, l, known));
            }
        }
        templates += 1;
        let op = o.expect("template has an opcode");
        if !ty_at.is_empty() {
            ty_at[pc] = (0..=255u8)
                .filter(|&s| cx.ty[s as usize] != Ty::Unk)
                .map(|s| (s, cx.ty[s as usize]))
                .collect();
        }
        let Some(w) = warm else {
            cx.template(pc as u32, i, op);
            continue;
        };
        // ---- tier 1.5 warm-up instrumentation (cfg.t15_warmup): the label-retargeting hook
        let start = cx.p.ops.len();
        let mut prof_post = None;
        let call_prof = op == OpCode::Call && i.c() >= 2;
        if op == OpCode::Call && w.in_warm_nest(pc as u32) {
            // intrinsic sites (task T6): the payload of a light C function in the callee slot, else 0
            let (l_skip, a) = (cx.p.label(), i.a());
            cx.op(Op::Mov(0, w.cprof_addr as u64 + 8 * pc as u64));
            cx.ld(1, W::B1, fr(a, 0));
            cx.op(Op::Mov(2, 0));
            cx.br(
                Cc::Ne,
                1,
                Src::Imm(i32::from(rn_core::value::LUA_VLCF)),
                l_skip,
            );
            cx.ld(2, W::B8, fr(a, 8));
            cx.op(Op::Bind(l_skip));
            cx.st(
                W::B8,
                M {
                    base: Base::Reg(0),
                    off: 0,
                },
                2,
            );
        }
        if (is_prof_load(op) || call_prof) && pc + 1 < n && w.in_warm_nest(pc as u32) {
            // The template falls through (or jumps) to `pcl[pc + 1]`: remap it to a label in front of the
            // profile store. Cold slow paths read `pcl[next]` later and rejoin behind it (their result tag is not
            // recorded: the profile only needs the fast path).
            let l = cx.p.label();
            prof_post = Some((l, cx.pcl[pc + 1]));
            cx.pcl[pc + 1] = l;
            if call_prof {
                // a compiled Lua callee returns through the resume table: that entry records the result too
                resume_alias.push((pc + 1, l));
            }
        }
        cx.template(pc as u32, i, op);
        if let Some((l, real)) = prof_post {
            cx.pcl[pc + 1] = real;
            cx.op(Op::Bind(l));
            cx.op(Op::Mov(0, w.prof_addr as u64 + pc as u64));
            cx.ld(1, W::B1, fr(i.a(), 0));
            cx.st(
                W::B1,
                M {
                    base: Base::Reg(0),
                    off: 0,
                },
                1,
            );
        }
        if let Some(&head) = be_head.get(&(pc as u32)) {
            if let Some(ni) = w.nest_of(pc as u32, head) {
                // every jump of the back-edge instruction to the loop head goes through the promote counter
                // (executed before the head's `CountSub`, M7)
                let from = cx.pcl[head as usize];
                let to = *warm_lbl.entry((ni, head)).or_insert_with(|| cx.p.label());
                if w.via_dispatch {
                    // J5/F5 (TR19b(b)): the edge keeps targeting `pcl[head]`, which the dispatch hook routes to `D_H`;
                    // the promote counter is the cell's `W_H` target
                    warm_by_head.entry(head).or_insert(to);
                    warm_deferred.push((start..cx.p.ops.len(), from, to, head));
                } else {
                    for op in &mut cx.p.ops[start..] {
                        retarget(op, from, to);
                    }
                }
            }
        }
    }
    // tier 1.5 regions: appended after the tier-1 code of every instruction
    let mut overrides: Vec<(u32, L)> = Vec::new();
    let mut resume_overrides: Vec<(u32, L)> = Vec::new();
    let mut points: Vec<RegionPoint> = Vec::new();
    if hook.is_none() {
        for (range, from, to, _) in &warm_deferred {
            for op in &mut cx.p.ops[range.clone()] {
                retarget(op, *from, *to);
            }
        }
    }
    if let Some(h) = hook.as_mut() {
        let follower: Vec<bool> = (0..n).map(|pc| cold[pc] || dead[pc]).collect();
        let main_len = cx.p.ops.len();
        let out = h(
            &mut cx,
            &RegionEnv {
                ins: &ins,
                ops: &ops,
                kinds: &kinds,
                head: &head[..n],
                cnt_at: &cnt_at,
                resume: &resume[..n],
                follower: &follower,
                ty_at: &ty_at,
                backedges: &backedges,
                warm_stub: &warm_by_head,
            },
        );
        for (range, from, to, head) in &warm_deferred {
            if !out.heads.iter().any(|&(h, _)| h == *head) {
                for op in &mut cx.p.ops[range.clone()] {
                    retarget(op, *from, *to);
                }
            }
        }
        // (P-G: one pass over the code for all heads; the stub labels are new, so no retarget chains. The `pcl` labels are
        // consecutive, so the table is indexed by label)
        let base = cx.pcl.first().copied().unwrap_or(0);
        let mut stub_of: Vec<L> = vec![NO_STUB; n];
        for &(hpc, stub) in &out.heads {
            debug_assert_eq!(cx.pcl[hpc as usize], base + hpc);
            stub_of[hpc as usize] = stub;
        }
        for op in &mut cx.p.ops[..main_len] {
            retarget_any(op, base, &stub_of);
        }
        // the fall-through into the head goes through the stub: the placeholder before its `Bind` becomes the jump (a head
        // without one, none today, gets the jump inserted, descending so the insertion points stay valid)
        let mut at: Vec<(usize, L)> = Vec::new();
        for &(hpc, stub) in &out.heads {
            let bi = bind_idx[hpc as usize];
            if bi == usize::MAX {
                continue;
            }
            if placeholder[hpc as usize] {
                cx.p.ops[bi - 1] = Op::Jmp(stub);
            } else {
                at.push((bi, stub));
            }
        }
        at.sort_unstable_by_key(|&(i, _)| std::cmp::Reverse(i));
        for (i, stub) in at {
            cx.p.ops.insert(i, Op::Jmp(stub));
        }
        overrides = out.overrides;
        resume_overrides = out.resume_overrides;
        points = out.points;
    }
    // out-of-line stubs of the MMBIN followers: the interpreter runs the arithmetic instruction first and
    // falls into them on the slow path
    for pc in cold_list {
        cx.op(Op::Bind(cx.pcl[pc]));
        cx.exit_at.insert((pc as u32, 0), cx.pcl[pc]);
        let addr = cx.pc_addr(pc as u32);
        cx.op(Op::Mov(0, addr));
        cx.st(W::B8, ciw(ci::SAVEDPC), 0);
        cx.op(Op::Ret(JitExit::Continue as u32));
    }
    cx.emit_cold();
    // P-X: the landing labels of the typed mid-block pcs: verify the tags tier 1 assumes there, else leave to the
    // interpreter at `pc` (refunding the pre-paid count, as a failing guard of the template at `pc` does)
    for (pc, l, known) in &land_checks {
        cx.op(Op::Bind(*l));
        #[cfg(feature = "trace")]
        let skip = crate::tr::snap::land_check_skipped();
        #[cfg(not(feature = "trace"))]
        let skip = false;
        if !skip {
            let x = cx.exit_r(*pc as u32);
            for &(slot, ty) in known {
                let tag = if ty == Ty::Int {
                    rn_core::value::LUA_VNUMINT
                } else {
                    rn_core::value::LUA_VNUMFLT
                };
                cx.ld(0, W::B1, fr(slot, 0));
                cx.br(Cc::Ne, 0, Src::Imm(i32::from(tag)), x);
            }
        }
        cx.op(Op::Jmp(cx.pcl[*pc]));
    }
    // lazily referenced exit sequences
    let mut at: Vec<((u32, u32), L)> = cx.exit_at.iter().map(|(&a, &b)| (a, b)).collect();
    at.sort_unstable();
    for ((pc, refund), l) in at {
        if refund == 0 && (kinds[pc as usize] == Kind::Stub) {
            continue; // emitted inline above
        }
        cx.op(Op::Bind(l));
        let addr = cx.pc_addr(pc);
        cx.op(Op::Mov(0, addr));
        cx.st(W::B8, ciw(ci::SAVEDPC), 0);
        if refund > 0 {
            // give back the pre-paid count of the instructions the interpreter will execute again
            let m = M {
                base: Base::Cnt,
                off: 0,
            };
            cx.ld(1, W::B4, m);
            cx.op(Op::Add(1, Src::Imm(refund as i32)));
            cx.st(W::B4, m, 1);
        }
        cx.op(Op::Ret(JitExit::Continue as u32));
    }
    let mut tr: Vec<(u32, L)> = cx.exit_trap.iter().map(|(&a, &b)| (a, b)).collect();
    tr.sort_unstable();
    for (pc, l) in tr {
        cx.op(Op::Bind(l));
        let addr = cx.pc_addr(pc);
        cx.op(Op::Mov(0, addr));
        cx.st(W::B8, ciw(ci::SAVEDPC), 0);
        cx.st_imm(W::B4, ciw(ci::TRAP), 1);
        cx.op(Op::Ret(JitExit::Continue as u32));
    }
    if let Some(w) = warm {
        let mut stubs: Vec<_> = warm_lbl.iter().map(|(&k, &l)| (k, l)).collect();
        stubs.sort_unstable();
        for ((ni, head), l) in stubs {
            cx.emit_warm_stub(w, ni, head, l);
        }
    }
    cx.emit_shared();
    debug_assert!(
        (1..n).all(|pc| !(resume[pc] && ops[pc - 1] == Some(OpCode::Call))
            || calls::valid_reentry(code, pc)),
        "a resume entry is not a valid re-entry point"
    );
    let ovr = |pc: usize, l: L| -> L {
        overrides
            .iter()
            .find(|&&(p, _)| p as usize == pc)
            .map_or(l, |&(_, s)| s)
    };
    let mut resume: Vec<(u32, L)> = (0..n)
        .filter(|&pc| resume[pc])
        .map(|pc| {
            let real = resume_alias
                .iter()
                .find(|&&(p, _)| p == pc)
                .map_or(cx.pcl[pc], |&(_, l)| l);
            let l = ovr(pc, real);
            let l = resume_overrides
                .iter()
                .find(|&&(p, _)| p as usize == pc)
                .map_or(l, |&(_, s)| s);
            (pc as u32, l)
        })
        .collect();
    // (BUNDLE-D: a continuation after an arithmetic instruction whose pc already is an entry, a loop head, uses that entry)
    // (a pc can be pushed more than once when an instruction is lowered twice, e.g. the main and the warm-up variant of an
    // arithmetic instruction: the first continuation wins, so that the entry list, which is part of the block signature
    // that every code version must reproduce (`BlockSig::resume_pcs`), has one entry per pc)
    let mut have: std::collections::HashSet<u32> = resume.iter().map(|&(pc, _)| pc).collect();
    resume.extend(
        cx.mm_resume
            .iter()
            .copied()
            .filter(|&(pc, _)| have.insert(pc)),
    );
    let resume_mm: Vec<(u32, L)> = cx
        .s1_resume
        .iter()
        .copied()
        .filter(|&(pc, _)| have.insert(pc))
        .collect();
    let pc_labels: Vec<Option<L>> = (0..n)
        .map(|pc| (!cold[pc] && !dead[pc]).then(|| ovr(pc, cx.pcl[pc])))
        .collect();
    let land_labels: Vec<Option<L>> = (0..n)
        .map(|pc| {
            (!cold[pc] && !dead[pc]).then(|| {
                let l = land_checks
                    .iter()
                    .find(|c| c.0 == pc)
                    .map_or(cx.pcl[pc], |c| c.1);
                ovr(pc, l)
            })
        })
        .collect();
    let sig = BlockSig {
        head: head[..n].to_vec(),
        cnt_at: cnt_at.clone(),
        rem: cx.rem.clone(),
        resume_pcs: resume.iter().map(|&(pc, _)| pc).collect(),
        entry_ok: kinds[0] != Kind::Stub,
        interp_entry: loop_total > 0
            || n >= 24
            || ops
                .iter()
                .any(|o| matches!(o, Some(OpCode::Call | OpCode::Tailcall))),
    };
    let mut prog = cx.p;
    prog.ops = peephole(std::mem::take(&mut prog.ops));
    Some(Lowered {
        points,
        pc_labels,
        land_labels,
        sig,
        backedges,
        prog,
        resume,
        resume_mm,
        templates,
        entry_ok: kinds[0] != Kind::Stub,
        interp_entry: loop_total > 0
            || n >= 24
            || ops
                .iter()
                .any(|o| matches!(o, Some(OpCode::Call | OpCode::Tailcall))),
        total: n,
        loop_total,
        loop_templates,
    })
}
