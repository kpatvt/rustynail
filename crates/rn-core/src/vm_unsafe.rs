//! The interpreter loop (`luaV_execute`, Lua 5.5; the few Lua 5.4 opcode forms are `cfg` arms): raw `pc` / `base` / `k` pointers and opcode
//! dispatch. This is the **only** `unsafe` interpreter module (see `docs/UNSAFE.md`, entry
//! "vm_unsafe.rs").
//!
//! # Invariants (V-series)
//! * **(VM1) code is trusted** (ADR-5): `pc` always points inside the running function's `code`
//!   array. The compiler/undump guarantee that the array ends with a `RETURN`, that every jump
//!   target is inside the array, and that every test/`NEWTABLE`/`SETLIST`/`LOADKX` is followed by
//!   the instruction the handler skips or reads.
//! * **(VM2) registers**: `base` points to `ci.func + 1` in the current thread's stack block and
//!   every register operand `a`, `b`, `c` (and the extra registers a handler touches) is
//!   `< maxstacksize`, which `pre_call`/`pre_tail_call` guarantee are inside the block
//!   (`ci.top <= stack_size`). `base` is stale after anything that can reallocate the stack
//!   (calls, metamethods, `check_stack`, GC finalizers); the `protect!` family and `reload!`
//!   refresh it, and debug builds assert on the stack generation counter at every register access.
//! * **(VM3) constants**: `k` points to `proto.k.len()` constants; operand indices are in range
//!   (compiler guarantee, VM1) and `K` operands used as table keys by `GETFIELD`/`SELF`/
//!   `GETTABUP`/`SETFIELD`/`SETTABUP` are short strings.
//! * **(VM4) closures**: `cl.upvals_ptr().add(n)` for `n < nupvalues`; slots are non-null and
//!   `UpVal.v` points to a live slot (F1-F3).
//!
//! The rest of the machinery (calls, errors, GC, hooks) is reached through safe `State` methods.
use crate::arith::{to_float, to_integer_ns};
use crate::error::{OpLoc, Opnd};
use crate::func::{LClosure, CLOSEKTOP};
use crate::instruction::Instruction;
#[cfg(rustynail_lua54)]
use crate::instruction::MAXARG_C;
#[cfg(not(rustynail_lua54))]
use crate::instruction::MAXARG_VC;
use crate::meta::Tm;
use crate::number::{self, Number, F2I};
use crate::state::*;
use crate::status::{LuaResult, Status};
use crate::table::{TableRef, HOK};
use crate::value::*;
use crate::ver::active::OpCode;

/// `tonumberns`: float value of a number (no string coercion).
#[inline(always)]
pub(crate) fn tonum(v: &Value) -> Option<f64> {
    match v.tag() {
        LUA_VNUMFLT => v.as_float(),
        LUA_VNUMINT => v.as_int().map(|i| i as f64),
        _ => None,
    }
}

/// A register operand (an index into the frame). Registers are addressed as `base` plus a byte
/// offset `index * size_of::<Value>()`: written that way (instead of `base.add(index)`), LLVM
/// folds the operand extraction and the scaling into one shift and one mask more often
/// (`(i >> 12) & 0xff0` for B): interpreter loops -1..-4% instructions.
trait Reg: Copy {
    /// Byte offset from `base`.
    fn off(self) -> usize;
    /// Register index.
    fn idx(self) -> usize;
}

const _: () = assert!(std::mem::size_of::<Value>() == 16);

impl Reg for usize {
    #[inline(always)]
    fn off(self) -> usize {
        self * 16
    }
    #[inline(always)]
    fn idx(self) -> usize {
        self
    }
}
impl Reg for u32 {
    #[inline(always)]
    fn off(self) -> usize {
        self as usize * 16
    }
    #[inline(always)]
    fn idx(self) -> usize {
        self as usize
    }
}

/// Levels of an `__index` chain of tables that the interpreter follows inline.
const INLINE_INDEX_DEPTH: usize = 4;

/// Fast part of `luaV_finishget` for a short-string key after a raw miss in table `t`: follows
/// `__index` fields that are tables (method lookup through classes) without the protected-call
/// bookkeeping of [`State::finish_get`]. Returns the final result (nil when the chain ends
/// without `__index`) or `ABSTKEY` for "take the general path" (an `__index` function, an
/// uncached absent `__index`, or a long chain). Out of line: inlining it into the interpreter
/// cost 3-5% instructions elsewhere (register allocation).
#[inline(never)]
pub(crate) fn index_chain_short_str(st: &State, t: TableRef, key: crate::string::StrRef) -> Value {
    let mut h = t;
    for _ in 0..INLINE_INDEX_DEPTH {
        let Some(mt) = h.metatable() else {
            return Value::NIL;
        };
        if mt.flags() & (1u8 << Tm::Index as u8) != 0 {
            return Value::NIL; // absence of `__index` cached in the flags
        }
        let tm = mt.get_short_str(st.g().heap.tm_name(Tm::Index));
        let Some(h2) = tm.as_table() else {
            return Value::ABSTKEY;
        };
        let v = h2.get_short_str(key);
        if !v.is_nil() {
            return v;
        }
        h = h2;
    }
    Value::ABSTKEY
}

/// `SELF` on a string (`s:sub(...)`): the method from the string metatable's `__index` table (and
/// its chain), `ABSTKEY` for the general path (no metatable, `__index` not a table, long chain).
#[inline(never)]
pub(crate) fn index_string_short_str(st: &State, key: crate::string::StrRef) -> Value {
    let Some(mt) = st.g().mt.get(LUA_TSTRING as usize).copied().flatten() else {
        return Value::ABSTKEY;
    };
    let tm = mt.get_short_str(st.g().heap.tm_name(Tm::Index));
    let Some(h) = tm.as_table() else {
        return Value::ABSTKEY;
    };
    let v = h.get_short_str(key);
    if !v.is_nil() {
        return v;
    }
    index_chain_short_str(st, h, key)
}

/// Slow path of `SETTABLE`/`SETI`/`SETFIELD`/`SETTABUP` (`luaV_finishset` with the operands of
/// instruction `i`). It re-reads the operands from the frame itself, so the fast paths never take
/// the address of their operand copies (which made LLVM spill them to the stack on every store).
#[cold]
#[inline(never)]
pub(crate) fn finish_set_op(st: &mut State, ci: Ci, i: Instruction, hres: i32) -> LuaResult<()> {
    let base = ci.r().func.0 + 1;
    let k = ci.r().kcache;
    // SAFETY: VM3 — constant indices are in range.
    let kv = |n: u32| unsafe { Value::load(k.add(n as usize)) };
    let a = i.a() as usize;
    let rc = if i.k() != 0 {
        kv(i.c())
    } else {
        st.get(StkId(base + i.c() as usize))
    };
    // SAFETY: VM1 — `i` is the instruction being executed.
    let op = unsafe { i.opcode().unwrap_unchecked() };
    let (t, key) = match op {
        OpCode::Settable => (
            Opnd::new(st.get(StkId(base + a)), OpLoc::Stack(StkId(base + a))),
            st.get(StkId(base + i.b() as usize)),
        ),
        OpCode::Seti => (
            Opnd::new(st.get(StkId(base + a)), OpLoc::Stack(StkId(base + a))),
            Value::int(i.b() as i64),
        ),
        OpCode::Setfield => (
            Opnd::new(st.get(StkId(base + a)), OpLoc::Stack(StkId(base + a))),
            kv(i.b()),
        ),
        _ => {
            debug_assert!(op == OpCode::Settabup);
            // SAFETY: VM2/VM4 — `base - 1` holds the running Lua closure.
            let upval = unsafe {
                let cl = st.get(StkId(base - 1)).as_lclosure().unwrap_unchecked();
                Value::load((**cl.upvals_ptr().add(a)).v)
            };
            (Opnd::new(upval, OpLoc::Upvalue(a as u8)), kv(i.b()))
        }
    };
    st.finish_set(t, &key, &rc, hres)
}

// ---- Lua metamethods run in place ---------------------------------------------------------
//
// A metamethod that is a Lua function is not called through `ccall` (a nested interpreter):
// the interpreter pushes its frame like `OP_CALL` would (`mm_frame`, marked `CIST_MMRET`) and
// continues with it in the same loop. When it returns, `do_ret!` sees the mark and `mm_return`
// completes the caller's instruction with the result, as `finish_op` does after a metamethod
// yielded (yields, errors and hooks inside the metamethod therefore behave as for any Lua call;
// `nCcalls` counts such frames as the reference counts its `luaD_call`). The launchers only
// cover the common cases (no hook, a Lua closure, enough stack and a spare `CallInfo`); they
// return `None` without side effects that matter otherwise, and the instruction takes its
// general slow path.

/// Push the frame of the Lua metamethod `f` with the arguments `args` at `func` (the caller's
/// `ci.top`, or its `top` for `CONCAT`), wanting `nresults` results.
#[inline(always)]
fn mm_frame(st: &mut State, func: StkId, f: Value, args: &[Value], nresults: i32) -> Option<Ci> {
    // Counted like the `luaD_call` it replaces (`nCcalls` + 1), so that unbounded metamethod
    // recursion still ends in "C stack overflow" at the same depth: at the limit the general
    // path (`ccall`) raises the error.
    let nc = st.ncalls_raw();
    if !f.is_lclosure() || st.hook_mask() != 0 || get_ccalls(nc) + 1 >= LUAI_MAXCCALLS {
        return None;
    }
    let old_top = st.top();
    // (`luaT_callTMres` also pushes its at most 4 values above `ci.top` without a stack check:
    // `EXTRA_STACK` covers them)
    st.set(func, f);
    for (n, v) in args.iter().enumerate() {
        st.set(func + 1 + n, *v);
    }
    st.set_top(func + 1 + args.len());
    match st.pre_call_lua_fast(func, nresults) {
        Some(nci) => {
            nci.w().callstatus |= CIST_MMRET;
            // (`ctx` is unused by Lua frames: `mm_return` undoes the increment)
            nci.w().ctx = nc as isize;
            st.set_ncalls_raw(nc + 1);
            Some(nci)
        }
        None => {
            st.set_top(old_top);
            None
        }
    }
}

/// `__index` function of table `t` (whose raw lookup of `key` found nothing).
#[inline(never)]
fn mm_index(st: &mut State, ci: Ci, t: Value, key: Value) -> Option<Ci> {
    // (BUNDLE-N: the chain of `luaV_finishget`: `__index` tables are walked with a raw lookup at each level; the first level
    // whose `__index` is a Lua function runs in place with that level's table as the first argument)
    let mut obj = t;
    let mut first = true;
    for _ in 0..crate::arith::MAXTAGLOOP {
        let h = obj.as_table()?;
        if !first {
            let v = match key.as_strref() {
                Some(ks) if key.is_short_string() => h.get_short_str(ks),
                _ => h.get(&key),
            };
            if !v.is_nil() {
                return None;
            }
        }
        let tm = st.fast_tm(h.metatable(), Tm::Index)?;
        first = false;
        if tm.is_lclosure() {
            return mm_frame(st, ci.r().top, tm, &[obj, key], 1);
        }
        if !tm.is_table() {
            return None;
        }
        obj = tm;
    }
    None
}

/// `__newindex` function for `SETTABLE`/`SETI`/`SETFIELD` (instruction `i`) after the raw store
/// into the table found no existing key (`hres != HOK`).
#[inline(never)]
fn mm_newindex(st: &mut State, ci: Ci, i: Instruction, t: Value) -> Option<Ci> {
    let base = ci.r().func.0 + 1;
    let h = t.as_table()?;
    let tm = st.fast_tm(h.metatable(), Tm::NewIndex)?;
    // SAFETY: VM3 — constant indices are in range.
    let kv = |n: u32| unsafe { Value::load(ci.r().kcache.add(n as usize)) };
    let val = if i.k() != 0 {
        kv(i.c())
    } else {
        st.get(StkId(base + i.c() as usize))
    };
    // SAFETY: VM1 — `i` is the instruction being executed.
    let key = match unsafe { i.opcode().unwrap_unchecked() } {
        OpCode::Settable => st.get(StkId(base + i.b() as usize)),
        OpCode::Seti => Value::int(i.b() as i64),
        _ => kv(i.b()),
    };
    mm_frame(st, ci.r().top, tm, &[t, key, val], 0)
}

/// Binary (`luaT_trybinTM`), unary (`p1 == p2`), order (`luaT_callorderTM`) and concatenation
/// metamethod `ev`: the first operand's, else the second's.
#[inline(never)]
fn mm_bin(st: &mut State, func: StkId, p1: Value, p2: Value, ev: Tm) -> Option<Ci> {
    let mut tm = st.get_tm_by_obj(&p1, ev);
    if tm.is_nil() {
        tm = st.get_tm_by_obj(&p2, ev);
    }
    mm_frame(st, func, tm, &[p1, p2], 1)
}

/// `__eq` of two tables or two full userdata that are not raw-equal (`luaV_equalobj`).
#[inline(never)]
fn mm_eq(st: &mut State, ci: Ci, a: Value, b: Value) -> Option<Ci> {
    let (m1, m2) = if let (Some(x), Some(y)) = (a.as_table(), b.as_table()) {
        (x.metatable(), y.metatable())
    } else if let (Some(x), Some(y)) = (a.as_userdata(), b.as_userdata()) {
        (x.metatable(), y.metatable())
    } else {
        return None;
    };
    let tm = match st.fast_tm(m1, Tm::Eq) {
        Some(tm) => tm,
        None => st.fast_tm(m2, Tm::Eq)?,
    };
    mm_frame(st, ci.r().top, tm, &[a, b], 1)
}

/// Tier 1 (`jit_cmp`): the in-place metamethod frame of an order comparison (`ev` = `Tm::Lt` / `Tm::Le`) of two values that are
/// not both numbers or both strings, exactly as the interpreter launches it (`mm_bin`); `None` = the interpreter's general path
/// must run the instruction (no metamethod, a C function, a hook, the C-stack limit, no room).
pub(crate) fn jit_mm_order(st: &mut State, ci: Ci, a: Value, b: Value, ev: Tm) -> Option<Ci> {
    mm_bin(st, ci.r().top, a, b, ev)
}

/// Tier 1 (`jit_cmp`): `__eq` of two tables / two full userdata that are not raw-equal. `Ok(Some(frame))`: the metamethod frame
/// was pushed; `Ok(None)`: no `__eq` metamethod at all (the values are unequal); `Err(())`: the interpreter must run the
/// instruction (a C function, a hook, the C-stack limit, no room).
pub(crate) fn jit_mm_eq(st: &mut State, ci: Ci, a: Value, b: Value) -> Result<Option<Ci>, ()> {
    let (m1, m2) = if let (Some(x), Some(y)) = (a.as_table(), b.as_table()) {
        (x.metatable(), y.metatable())
    } else if let (Some(x), Some(y)) = (a.as_userdata(), b.as_userdata()) {
        (x.metatable(), y.metatable())
    } else {
        return Err(());
    };
    let tm = match st.fast_tm(m1, Tm::Eq) {
        Some(tm) => tm,
        None => match st.fast_tm(m2, Tm::Eq) {
            Some(tm) => tm,
            None => return Ok(None),
        },
    };
    match mm_frame(st, ci.r().top, tm, &[a, b], 1) {
        Some(nci) => Ok(Some(nci)),
        None => Err(()),
    }
}

/// BUNDLE-S S1 (tier 1, `jit_get`): the in-place frame of the Lua `__index` function `tm` found at the level `obj` of a chain (the table whose metatable holds
/// it, or a non-table operand with a type metatable) for the key `key`, wanting one result: `luaV_finishget`'s call `tm(obj, key)`, launched as the interpreter's
/// `mm_index` does. The caller has saved `savedpc` = the instruction after the read. `false` = nothing happened (a hook, the C-stack limit, no room): the
/// interpreter runs the instruction.
pub(crate) fn jit_mm_index(st: &mut State, ci: Ci, tm: Value, obj: Value, key: Value) -> bool {
    mm_frame(st, ci.r().top, tm, &[obj, key], 1).is_some()
}

/// BUNDLE-S S1 (tier 1, `jit_set`): the in-place frame of the Lua `__newindex` of the table `t` for the store instruction `i` of the running frame `ci` (the
/// interpreter's `mm_newindex`, results discarded). `false` = nothing happened (no Lua function, a hook, the C-stack limit, no room).
pub(crate) fn jit_mm_newindex(st: &mut State, ci: Ci, i: Instruction, t: Value) -> bool {
    mm_newindex(st, ci, i, t).is_some()
}

/// `jit_mm_pop` mutant switch; always false without the `jit` feature.
#[inline(always)]
fn mm_ret_mut(bit: u8) -> bool {
    #[cfg(feature = "jit")]
    return crate::jit_helpers::mut_mm_ret(bit);
    #[cfg(not(feature = "jit"))]
    {
        let _ = bit;
        false
    }
}

/// Tier 1 (`jit_mm_ret`): the metamethod frame `ret` (a `RETURN0` / `RETURN1` of compiled code) returned `cond` to a comparison of
/// its caller. Pops the frame like `mm_return` does for a comparison, but leaves the caller's `savedpc` on the instruction after the
/// comparison (its `JMP`) and stores the comparison's result as a boolean at the frame's old function slot (the caller's `top`):
/// compiled code resumes through the resume table entry of the `JMP` and reads the flag, the interpreter (when the caller has no
/// entry) finishes the instruction with [`jit_mm_finish`]. `false` (nothing changed) when the caller's instruction is not one of the
/// register comparisons.
pub(crate) fn jit_mm_pop(st: &mut State, ret: Ci, v: Value, flags: u32) -> bool {
    let arith = flags & 1 != 0;
    let Some(caller) = ret.previous() else {
        return false;
    };
    #[cfg(rustynail_lua54)]
    if caller.r().callstatus & CIST_LEQ != 0 {
        return false;
    }
    if caller.r().callstatus & CIST_C != 0 {
        return false;
    }
    // SAFETY: VM1 — the caller was interrupted after fetching an instruction (it called the metamethod).
    let inst = Instruction(unsafe { *caller.r().savedpc.sub(1) });
    let op = inst.opcode();
    if arith
        && matches!(
            op,
            Some(OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink | OpCode::Unm | OpCode::Bnot)
        )
    {
        // BUNDLE-D (`armm`): the metamethod of an arithmetic instruction returned `v`: `mm_return`'s store into the register of the
        // arithmetic instruction (the one before the `MMBIN*`, or the unary instruction itself); `savedpc` stays after the `MMBIN*`
        let dst = if matches!(op, Some(OpCode::Unm | OpCode::Bnot)) {
            inst.a()
        } else {
            // SAFETY: an MMBIN* instruction is always preceded by its arithmetic instruction.
            Instruction(unsafe { *caller.r().savedpc.sub(2) }).a()
        };
        let nc = st.ncalls_raw();
        if nc as isize == ret.r().ctx + 1 && !mm_ret_mut(1) {
            st.set_ncalls_raw(nc - 1);
        }
        let res = ret.r().func;
        let base = caller.r().func.0 + 1;
        let dst = if mm_ret_mut(4) { dst + 1 } else { dst };
        st.set(StkId(base + dst as usize), v);
        st.set_top(res);
        st.set_ci(caller);
        return true;
    }
    // BUNDLE-S S1 (`flags` bit 1 / 2): the metamethod of a table read (`__index` function) or store (`__newindex` function) returned: `mm_return`'s store of the
    // result into `R[A]` (nothing for a store); `savedpc` stays after the instruction
    let is_get = matches!(
        op,
        Some(
            OpCode::Gettabup | OpCode::Gettable | OpCode::Geti | OpCode::Getfield | OpCode::SelfOp
        )
    );
    let is_set = matches!(
        op,
        Some(OpCode::Settabup | OpCode::Settable | OpCode::Seti | OpCode::Setfield)
    );
    if (flags & 2 != 0 && is_get) || (flags & 4 != 0 && is_set) {
        let nc = st.ncalls_raw();
        if nc as isize == ret.r().ctx + 1 && !mm_ret_mut(1) {
            st.set_ncalls_raw(nc - 1);
        }
        let res = ret.r().func;
        if is_get {
            let base = caller.r().func.0 + 1;
            let dst = if flags & 0x100 != 0 {
                inst.a() + 1
            } else {
                inst.a()
            }; // (mutant `s1_get_dst_off`)
            st.set(StkId(base + dst as usize), v);
        }
        st.set_top(res);
        st.set_ci(caller);
        return true;
    }
    if !matches!(op, Some(OpCode::Lt | OpCode::Le | OpCode::Eq)) {
        return false;
    }
    let nc = st.ncalls_raw();
    if nc as isize == ret.r().ctx + 1 && !mm_ret_mut(1) {
        st.set_ncalls_raw(nc - 1);
    }
    let res = ret.r().func;
    let cond = !v.is_falsy() && !mm_ret_mut(2);
    st.set(res, if cond { Value::TRUE } else { Value::FALSE });
    st.set_top(res);
    st.set_ci(caller);
    true
}

/// BUNDLE-D (`armm`): launch the in-place frame of the Lua metamethod of the arithmetic instruction `instr` of the running frame `ci`
/// exactly as the interpreter would at its `MMBIN*` (`instr` is the `MMBIN*` word; its `A`/`B` operands are the instruction's) or at the
/// unary instruction itself (`UNM` / `BNOT`). The caller has saved `savedpc` = the instruction after `instr`. `false` = nothing happened
/// (no Lua metamethod, a hook, the C-stack limit, no room): the interpreter runs the arithmetic instruction itself.
pub(crate) fn jit_mm_arith(st: &mut State, ci: Ci, instr: Instruction) -> bool {
    let base = ci.r().func.0 + 1;
    let rd = |n: u32| st.get(StkId(base + n as usize));
    // SAFETY: VM3 — constant indices are in range.
    let kv = |n: u32| unsafe { Value::load(ci.r().kcache.add(n as usize)) };
    let top = ci.r().top;
    let r = match instr.opcode() {
        Some(OpCode::Mmbin) => {
            let (a, b) = (rd(instr.a()), rd(instr.b()));
            mm_bin(st, top, a, b, Tm::from_u8(instr.c() as u8))
        }
        Some(OpCode::Mmbini) => {
            let (v1, v2) = (rd(instr.a()), Value::int(i64::from(instr.sb())));
            let (q1, q2) = if instr.k() != 0 { (v2, v1) } else { (v1, v2) };
            mm_bin(st, top, q1, q2, Tm::from_u8(instr.c() as u8))
        }
        Some(OpCode::Mmbink) => {
            let (v1, v2) = (rd(instr.a()), kv(instr.b()));
            let (q1, q2) = if instr.k() != 0 { (v2, v1) } else { (v1, v2) };
            mm_bin(st, top, q1, q2, Tm::from_u8(instr.c() as u8))
        }
        Some(OpCode::Unm) => {
            let rb = rd(instr.b());
            mm_bin(st, top, rb, rb, Tm::Unm)
        }
        Some(OpCode::Bnot) => {
            let rb = rd(instr.b());
            mm_bin(st, top, rb, rb, Tm::BNot)
        }
        _ => None,
    };
    r.is_some()
}

/// Tier 1: finish the comparison of the current frame whose result [`jit_mm_pop`] left at its `top` (the interpreter's `mm_return`
/// tail: skip the `JMP` when the condition fails).
pub(crate) fn jit_mm_finish(st: &mut State) {
    let caller = st.ci();
    // SAFETY: VM1 — as in `jit_mm_pop`; the comparison is followed by a `JMP`.
    let inst = Instruction(unsafe { *caller.r().savedpc.sub(1) });
    if !matches!(inst.opcode(), Some(OpCode::Lt | OpCode::Le | OpCode::Eq)) {
        return; // (BUNDLE-D `armm`: an arithmetic instruction: `jit_mm_pop` stored the result, `savedpc` is already after it)
    }
    let cond = !st.get(caller.r().top).is_falsy();
    if (cond as u32) != inst.k() {
        // SAFETY: VM1 — the comparison is followed by a JMP.
        caller.w().savedpc = unsafe { caller.r().savedpc.add(1) };
    }
}

/// BUNDLE-V (`jit_tailcall`): may the `TAILCALL` of the metamethod frame `ci` (callee value `f`) run in compiled code? A Lua closure always (the frame stays the metamethod
/// frame, its return goes through `jit_mm_ret`); anything else (a native function, a table with `__call`) only when the metamethod belongs to an arithmetic / bitwise /
/// unary / comparison instruction of the caller: native tail callees in `__index` / `__newindex` frames (`return rawget(t, k)`) moved the trace formation of
/// `class_middleclass` by +7 % cycles (-3 % instructions) and stay with the interpreter as before.
pub(crate) fn jit_mm_tail_ok(st: &State, ci: Ci, f: Value) -> bool {
    let _ = st;
    if f.is_lclosure() {
        return true;
    }
    let Some(caller) = ci.previous() else {
        return false;
    };
    if caller.r().callstatus & CIST_C != 0 {
        return false;
    }
    // SAFETY: VM1 — the caller was interrupted after fetching the instruction that called the metamethod.
    let inst = Instruction(unsafe { *caller.r().savedpc.sub(1) });
    matches!(
        inst.opcode(),
        Some(
            OpCode::Mmbin
                | OpCode::Mmbini
                | OpCode::Mmbink
                | OpCode::Unm
                | OpCode::Bnot
                | OpCode::Lt
                | OpCode::Le
                | OpCode::Eq
        )
    )
}

/// Tier 1 (`jit_tailcall`): the interpreter's `mm_return` for the popped metamethod frame `ret` whose native tail callee finished.
pub(crate) fn jit_mm_return(st: &mut State, ret: Ci) -> LuaResult<()> {
    mm_return(st, ret).map(|_| ())
}

/// A metamethod frame run in place (`CIST_MMRET`) returned to its caller (the current frame);
/// `ret` is the returned frame, whose function slot holds the result (if one was wanted). Finish
/// the caller's instruction like `finish_op` (which handles the rarer cases) and return the
/// caller's frame, which continues at its saved `pc`.
#[inline(never)]
fn mm_return(st: &mut State, ret: Ci) -> LuaResult<Ci> {
    let caller = st.ci();
    debug_assert_eq!(Some(caller.as_ptr()), ret.previous().map(|c| c.as_ptr()));
    // undo `mm_frame`'s `nCcalls` increment, unless the count was reset meanwhile (a yield
    // inside the metamethod: `resume` restarts the count, as the reference's discarded C frames
    // of the call are not counted after a resume either)
    let nc = st.ncalls_raw();
    if nc as isize == ret.r().ctx + 1 {
        st.set_ncalls_raw(nc - 1);
    }
    let res = ret.r().func;
    let base = caller.r().func.0 + 1;
    // SAFETY: VM1 — the caller was interrupted after fetching an instruction.
    let inst = Instruction(unsafe { *caller.r().savedpc.sub(1) });
    // SAFETY: VM1 — a valid instruction.
    match unsafe { inst.opcode().unwrap_unchecked() } {
        OpCode::Gettabup
        | OpCode::Gettable
        | OpCode::Geti
        | OpCode::Getfield
        | OpCode::SelfOp
        | OpCode::Unm
        | OpCode::Bnot
        | OpCode::Len => {
            let v = st.get(res);
            st.set(StkId(base + inst.a() as usize), v);
            st.set_top(res);
        }
        OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink => {
            // SAFETY: an MMBIN* instruction is always preceded by its arithmetic instruction.
            let prev = Instruction(unsafe { *caller.r().savedpc.sub(2) });
            let v = st.get(res);
            st.set(StkId(base + prev.a() as usize), v);
            st.set_top(res);
        }
        OpCode::Lt
        | OpCode::Le
        | OpCode::Lti
        | OpCode::Lei
        | OpCode::Gti
        | OpCode::Gei
        | OpCode::Eq => {
            let cond = !st.get(res).is_falsy();
            st.set_top(res);
            if (cond as u32) != inst.k() {
                // condition failed: skip the jump instruction
                // SAFETY: VM1 — the comparison is followed by a JMP.
                caller.w().savedpc = unsafe { caller.r().savedpc.add(1) };
            }
        }
        _ => finish_op(st)?, // CONCAT; SET*: nothing to finish
    }
    Ok(caller)
}

/// `luaT_getvarargs` for a function without a vararg table: copy the extra arguments, which
/// sit right below the function slot (`luaT_adjustvarargs`), into the `n` slots from `where_`
/// (nil-completed). The caller guarantees the room: registers of the frame for a fixed count,
/// `check_stack(n)` for all of them. Cannot fail or move the stack.
#[inline(always)]
pub(crate) fn varargs_fixed(st: &mut State, ci: Ci, where_: StkId, n: usize) {
    let nextra = ci.r().nextraargs as usize;
    let func = ci.r().func;
    assert!(
        func.0 >= nextra && where_.0 + n <= st.stack_size(),
        "vararg copy out of the stack"
    );
    let touse = nextra.min(n);
    // SAFETY: VM2 — the `nextra` slots below `func` hold the extra arguments and the `n`
    // destination slots are inside the stack block (checked above); source and destination
    // are disjoint (below and above `func`).
    unsafe {
        let src = st.slot_ptr(StkId(func.0 - nextra));
        let dst = st.slot_ptr(where_);
        for j in 0..touse {
            Value::store(dst.add(j), Value::load(src.add(j)));
        }
        for j in touse..n {
            Value::store(dst.add(j), Value::NIL);
        }
    }
}

/// `OP_VARARGPREP` (`luaT_adjustvarargs`) for a function without a vararg table: copy the
/// function and the fixed parameters above the extra arguments (stack pointer read once).
#[inline(always)]
pub(crate) fn adjust_varargs_fast(
    st: &mut State,
    ci: Ci,
    p: Gc<crate::func::Proto>,
) -> LuaResult<()> {
    let func = ci.r().func;
    let top = st.top();
    let totalargs = top - func - 1;
    let nfix = p.numparams as usize;
    debug_assert!(totalargs >= nfix);
    ci.w().nextraargs = (totalargs - nfix) as i32;
    st.check_stack(p.maxstacksize as usize + 1)?;
    // SAFETY: `func..top` are the function and its arguments; `check_stack` made room for the
    // `nfix + 1` copies above `top` (the frame's `maxstacksize + 1` slots).
    unsafe {
        let f = st.slot_ptr(func);
        let t = st.slot_ptr(top);
        Value::store(t, Value::load(f)); // the function
        for j in 1..=nfix {
            Value::store(t.add(j), Value::load(f.add(j)));
            Value::store(f.add(j), Value::NIL); // erase original parameter (for GC)
        }
        // set the vararg parameter slot to nil
        Value::store(t.add(nfix + 1), Value::NIL);
    }
    st.set_top(top + nfix + 1);
    let c = ci.w();
    c.func += totalargs + 1;
    c.top += totalargs + 1;
    debug_assert!(st.top() <= ci.r().top && ci.r().top.0 <= st.stack_size());
    Ok(())
}

/// 5.4 `forprep` when the initial value, the limit and the step are integers and the step is not
/// zero (the common loop): `Some(skip)` with the loop slots prepared, `None` for the general path
/// ([`State::for_prep`], which also raises the errors). One stack-pointer read instead of one per
/// slot access.
#[cfg(rustynail_lua54)]
#[inline(always)]
pub(crate) fn forprep_int54(st: &mut State, ra: StkId) -> Option<bool> {
    assert!(
        ra.0 + 4 <= st.stack_size(),
        "for loop slots out of the stack"
    );
    let p = st.slot_ptr(ra);
    // SAFETY: `ra..ra + 4` are slots of the stack block (checked above) holding initialised
    // values; payloads are read as integers only after their tags say so.
    unsafe {
        if Value::load_tag(p) != LUA_VNUMINT
            || Value::load_tag(p.add(1)) != LUA_VNUMINT
            || Value::load_tag(p.add(2)) != LUA_VNUMINT
        {
            return None;
        }
        let init = Value::load_int_payload(p);
        let limit = Value::load_int_payload(p.add(1));
        let step = Value::load_int_payload(p.add(2));
        if step == 0 {
            return None; // "'for' step is zero"
        }
        Value::store(p.add(3), Value::int(init)); // control variable
        let skip = if step > 0 { init > limit } else { init < limit };
        if skip {
            return Some(true);
        }
        // `forprep`'s iteration count (the limit slot becomes the counter)
        let count = if step > 0 {
            let c = (limit as u64).wrapping_sub(init as u64);
            if step != 1 {
                c / step as u64
            } else {
                c
            }
        } else {
            let c = (init as u64).wrapping_sub(limit as u64);
            c / ((step.wrapping_add(1)).wrapping_neg() as u64).wrapping_add(1)
        };
        Value::store_int_payload(p.add(1), count as i64);
        Some(false)
    }
}

#[cold]
#[inline(never)]
fn bad_frame(st: &mut State) -> Status {
    st.run_error(b"internal error: not a Lua frame")
}

/// Run the function of frame `ci` (and every Lua function it calls) until the frame marked
/// `CIST_FRESH` returns.
///
/// The interpreter exists in two copies. The *fast* one (`HOOK = false`) never tests `trap`
/// per instruction: `trap` can only become set inside a call, a metamethod, a collection or a
/// stack reallocation (all go through `protect!`/`check_gc!` or a frame transition), by an
/// asynchronous `lua_sethook` (noticed at jumps and calls, like the reference's `updatetrap`)
/// or by a pending interrupt (polled at the same points). When one of those points finds `trap`
/// set for a reason that needs per-instruction tracing (hooks, interrupt), the fast copy ends the
/// current instruction and hands the frame to the *hooked* copy (`HOOK = true`, the reference's
/// `vmfetch` with its `trap` test), which hands it back at a frame transition once no hook is left.
pub(crate) fn run(st: &mut State, ci0: Ci) -> LuaResult<()> {
    if st.hook_mask() != 0 {
        ci0.w().trap = 1; // `startfunc`: trap = L->hookmask
    }
    // P-O: a Lua function called by an inlined `pcall` runs in a fresh `execute`: the recorder expects its first step
    #[cfg(feature = "jit_trace")]
    if crate::jit_hooks::rec_armed() && crate::jit_hooks::rec_wants(st, ci0) {
        ci0.w().trap = 1;
    }
    // J5 recorder (TR23.5): the one error choke point of recorded frames. It covers both copies: a recording armed
    // in the fast copy (`rec_arm`) can meet an error before its first hooked step.
    #[cfg(feature = "jit_trace")]
    let mut next = {
        let r = run_mode::<false>(st, ci0, false);
        rec_on_err(st, &r);
        r?
    };
    #[cfg(not(feature = "jit_trace"))]
    let mut next = run_mode::<false>(st, ci0, false)?;
    let mut hooked = true;
    while let Some((ci, resume)) = next {
        #[cfg(feature = "jit_trace")]
        {
            let r = if hooked {
                run_mode::<true>(st, ci, resume)
            } else {
                run_mode::<false>(st, ci, resume)
            };
            rec_on_err(st, &r);
            next = r?;
        }
        #[cfg(not(feature = "jit_trace"))]
        {
            next = if hooked {
                run_mode::<true>(st, ci, resume)?
            } else {
                run_mode::<false>(st, ci, resume)?
            };
        }
        hooked = !hooked;
    }
    Ok(())
}

/// TR23.5: a copy of the interpreter returned `Err`: abort the recording armed for this state, if any. A yield
/// (unwinding through `Err(Status::Yield)`) is a frame switch, not an instruction error.
#[cfg(feature = "jit_trace")]
#[inline(always)]
fn rec_on_err<T>(st: &mut State, r: &LuaResult<T>) {
    if let Err(e) = r {
        if crate::jit_hooks::rec_active() {
            let why = if *e == Status::Yield {
                crate::jit_hooks::Abort::Frame
            } else {
                crate::jit_hooks::Abort::Error
            };
            crate::jit_hooks::rec_abort(st, why);
        }
    }
}

/// Fast-mode `updatetrap`, out of line: `trap` or the interrupt flag is set. Returns whether the
/// frame must continue in the hooked interpreter; a `trap` left by a stack reallocation alone
/// (the interpreter reloads `base` after every call anyway) is just cleared.
#[cold]
#[inline(never)]
fn fast_trap(st: &mut State, ci: Ci) -> bool {
    // J5 recorder (TR23.1): while a recording is armed (any thread) a trapped frame goes to the hooked copy. A
    // plain load of a static, like `INTERRUPT`: the attributes LLVM infers for this function (`st` read-only, only
    // `ci.trap` written) must not change, or the fast copy is compiled differently (TR23.1 gate, scripts/jit-text-identity.sh).
    #[cfg(feature = "jit_trace")]
    if crate::jit_hooks::rec_armed() {
        ci.w().trap = 1;
        return true;
    }
    if crate::state::INTERRUPT.load(std::sync::atomic::Ordering::Relaxed) || st.hook_mask() != 0 {
        ci.w().trap = 1;
        true
    } else {
        ci.w().trap = 0;
        false
    }
}

/// One copy of the interpreter (see [`run`]). Returns `None` when the `CIST_FRESH` frame returned,
/// or `Some((ci, resume))` to continue frame `ci` in the other copy: `resume` means "in the middle
/// of the function at `ci.savedpc`" (no call hook), otherwise the frame is (re)started.
#[allow(unused_assignments)]
fn run_mode<const HOOK: bool>(
    st: &mut State,
    ci0: Ci,
    mut resume: bool,
) -> LuaResult<Option<(Ci, bool)>> {
    let mut ci = ci0;
    'frame: loop {
        // ---- (re)start the function of `ci` (`startfunc` / `returning`) -----------------------
        if HOOK {
            // J5 recorder (TR23.2): keep the frame the recorder expects (a callee it just recorded a CALL to) in
            // the hooked copy; before the `TRAP_JIT` rewrite and the "no hooks left" return below.
            #[cfg(feature = "jit_trace")]
            if crate::jit_hooks::rec_active() {
                crate::jit_hooks::rec_head(st, ci);
            }
            // JIT entry flag meets the hooked copy: it is not an entry point here
            #[cfg(feature = "jit")]
            if ci.r().trap == crate::jit_hooks::TRAP_JIT {
                ci.w().trap = i32::from(st.hook_mask() != 0);
            }
            if resume {
                resume = false;
            } else if ci.r().trap != 0 {
                st.trace_call()?;
            } else if st.hook_mask() == 0 {
                return Ok(Some((ci, false))); // no hooks left: back to the fast interpreter
            }
        } else {
            // JIT (docs/JIT_DESIGN.md section 2): `jit_hot_call` / the loop hook flagged this frame
            // for entry into compiled code at `savedpc`. Hooks and interrupts take precedence.
            #[cfg(feature = "jit")]
            if ci.r().trap == crate::jit_hooks::TRAP_JIT
                && st.hook_mask() == 0
                && !crate::state::INTERRUPT.load(std::sync::atomic::Ordering::Relaxed)
            {
                ci.w().trap = 0;
                match crate::jit_hooks::enter(st, ci) {
                    crate::jit_hooks::JitExit::Continue => {}
                    crate::jit_hooks::JitExit::FrameChanged => {
                        ci = st.ci();
                        continue 'frame;
                    }
                    crate::jit_hooks::JitExit::Returned => return Ok(None),
                    crate::jit_hooks::JitExit::Error => {
                        return Err(crate::jit_hooks::take_error(st));
                    }
                }
            }
            if ci.r().trap != 0 && fast_trap(st, ci) {
                return Ok(Some((ci, false))); // hooks: the hooked interpreter runs the call hook
            }
        }
        let mut pc: *const u32 = ci.r().savedpc;
        let mut trapped = false;
        let mut base: *mut Value = st.slot_ptr(StkId(ci.r().func.0 + 1));
        // The function slot sits right below the base. `ci.kcache` was set when the frame was
        // created (`pre_call*`, `pre_tail_call`) and stays valid: returns need not re-derive it.
        // SAFETY: VM2 — `func` is a valid slot, so `base - 1` is in the stack block.
        if unsafe { Value::load_tag(base.sub(1)) } != ctb(LUA_VLCL) {
            return Err(bad_frame(st));
        }
        debug_assert_eq!(
            ci.r().kcache,
            // SAFETY: as above.
            unsafe { Value::load(base.sub(1)) }
                .as_lclosure()
                .map(|c| c.proto().k_ptr())
                .unwrap()
        );
        let mut gen = st.stack_gen();
        // JIT hooks (feature `jit`): entry count (placement c) and the E1 entry check.
        #[cfg(all(
            feature = "jit",
            not(feature = "jit_check_d"),
            not(feature = "jit_check_e"),
            not(feature = "jit_no_entry")
        ))]
        if !HOOK {
            // SAFETY: VM2/VM4 — `base - 1` holds the frame's Lua closure (checked above).
            let p = unsafe { Value::load(base.sub(1)).as_lclosure().unwrap_unchecked() }.proto();
            #[cfg(not(feature = "jit_counter_b"))]
            {
                let h = p.hot.get() - crate::jit_hooks::HOT_ENTRY_WEIGHT;
                p.hot.set(h);
                if h <= 0 {
                    crate::jit_hooks::jit_hot(st, Some(&*p));
                }
            }
            #[cfg(feature = "jit_counter_b")]
            if st.jit_tick_dec() {
                crate::jit_hooks::jit_hot(st, None);
            }
            if !p.jit.get().is_null() && crate::jit_hooks::jit_try_enter(st, ci) {
                continue 'frame;
            }
        }

        // ---- helper macros (they capture the locals above) -------------------------------------
        // The running closure and its prototype are not kept in registers across the loop (the
        // interpreter has few callee-saved registers): they are re-derived from the frame.
        macro_rules! lcl {
            () => {{
                debug_assert_eq!(st.stack_gen(), gen, "stale base pointer");
                // SAFETY: VM2/VM4 — `base - 1` is the frame's function slot, which holds a Lua
                // closure (one load from the register base instead of re-deriving the slot).
                #[allow(unused_unsafe)]
                unsafe {
                    Value::load(base.sub(1)).as_lclosure().unwrap_unchecked()
                }
            }};
        }
        macro_rules! reload {
            () => {{
                base = st.slot_ptr(StkId(ci.r().func.0 + 1));
                gen = st.stack_gen();
            }};
        }
        macro_rules! rd {
            ($n:expr) => {{
                let n = $n;
                debug_assert_eq!(st.stack_gen(), gen, "stale base pointer");
                debug_assert!(ci.r().func.0 + 1 + Reg::idx(n) < st.stack_size() + EXTRA_STACK);
                // SAFETY: VM2 — `n` is a register of this frame, inside the stack block.
                unsafe { Value::load(base.byte_add(Reg::off(n))) }
            }};
        }
        macro_rules! wr {
            ($n:expr, $v:expr) => {{
                let v: Value = $v;
                let n = $n;
                debug_assert_eq!(st.stack_gen(), gen, "stale base pointer");
                debug_assert!(ci.r().func.0 + 1 + Reg::idx(n) < st.stack_size() + EXTRA_STACK);
                // SAFETY: VM2.
                unsafe { Value::store(base.byte_add(Reg::off(n)), v) }
            }};
        }
        macro_rules! kv {
            ($n:expr) => {{
                // SAFETY: VM3 — constant index in range.
                unsafe { Value::load(ci.r().kcache.add($n as usize)) }
            }};
        }
        macro_rules! sidx {
            ($n:expr) => {
                StkId(ci.r().func.0 + 1 + Reg::idx($n))
            };
        }
        macro_rules! pc_add {
            ($n:expr) => {{
                // SAFETY: VM1 — the target stays inside the code array.
                pc = unsafe { pc.offset($n as isize) };
            }};
        }
        macro_rules! fetch_next {
            () => {{
                // SAFETY: VM1 — a following instruction exists.
                Instruction(unsafe { *pc })
            }};
        }
        macro_rules! savepc {
            () => {
                ci.w().savedpc = pc
            };
        }
        macro_rules! savestate {
            () => {{
                savepc!();
                let t = ci.r().top;
                st.set_top(t);
            }};
        }
        macro_rules! updatetrap {
            () => {
                if HOOK {
                    // a pending Ctrl-C makes the next instruction run `trace_exec`
                    if crate::state::INTERRUPT.load(std::sync::atomic::Ordering::Relaxed) {
                        ci.w().trap = 1;
                    }
                } else if (ci.r().trap != 0
                    || crate::state::INTERRUPT.load(std::sync::atomic::Ordering::Relaxed))
                    && fast_trap(st, ci)
                {
                    trapped = true; // switch interpreters once this instruction is done
                }
            };
        }
        // `Protect`: may raise errors, reallocate the stack, change hooks.
        macro_rules! protect {
            ($e:expr) => {{
                savestate!();
                let r = $e;
                // `pc` was saved just above: re-reading it makes it dead across the call, so it
                // need not be kept in a callee-saved register (or a stack slot) meanwhile
                pc = ci.r().savedpc;
                reload!();
                updatetrap!();
                r?
            }};
        }
        // `ProtectNT`: like `protect!` but does not change `top`.
        macro_rules! protect_nt {
            ($e:expr) => {{
                savepc!();
                let r = $e;
                pc = ci.r().savedpc;
                reload!();
                updatetrap!();
                r?
            }};
        }
        // `halfProtect`: can only raise errors.
        macro_rules! half_protect {
            ($e:expr) => {{
                savestate!();
                $e?
            }};
        }
        // `checkGC`: `$lim` is the first dead stack index.
        macro_rules! check_gc {
            ($lim:expr) => {{
                if st.g().heap.gc_due() {
                    savepc!();
                    st.set_top($lim);
                    st.check_gc();
                    reload!();
                    updatetrap!();
                }
            }};
        }
        // JIT hotness at a loop back-edge (counter A: `Proto.hot`; B: the thread's sampling tick).
        macro_rules! hot_edge {
            () => {{
                #[cfg(all(
                    feature = "jit",
                    not(feature = "jit_counter_b"),
                    not(feature = "jit_counter_c"),
                    not(feature = "jit_no_loop")
                ))]
                if !HOOK {
                    let p = lcl!().proto();
                    let h = p.hot.get() - 1;
                    p.hot.set(h);
                    if h <= 0 {
                        crate::jit_hooks::jit_hot(st, Some(&*p));
                    }
                }
                #[cfg(all(feature = "jit_counter_b", not(feature = "jit_no_loop")))]
                if !HOOK && st.jit_tick_dec() {
                    // sampling tick: compile the running function if needed and continue this frame
                    // in compiled code (OSR) when `pc` is an entry point of it
                    if crate::jit_hooks::jit_hot_loop(st, ci, pc) {
                        ci.w().savedpc = pc;
                        ci.w().trap = crate::jit_hooks::TRAP_JIT;
                        continue 'frame;
                    }
                }
                #[cfg(all(feature = "jit_counter_c", not(feature = "jit_no_loop")))]
                if !HOOK {
                    let c = ci.w();
                    c.hot -= 1;
                    if c.hot <= 0 {
                        crate::jit_hooks::jit_hot_loop_c(st, ci);
                    }
                }
            }};
        }
        macro_rules! dojump {
            ($sj:expr, $e:expr) => {{
                pc_add!(($sj as isize) + ($e as isize));
                #[cfg(all(feature = "jit", not(feature = "jit_no_loop")))]
                if ($sj as isize) < 0 {
                    hot_edge!();
                }
                updatetrap!();
            }};
        }
        macro_rules! donextjump {
            () => {{
                let ni = fetch_next!();
                dojump!(ni.sj(), 1);
            }};
        }
        macro_rules! docondjump {
            ($i:expr, $cond:expr) => {{
                if (($cond) as u32) != $i.k() {
                    pc_add!(1);
                } else {
                    donextjump!();
                }
            }};
        }
        // Where a `ret:` label would be.
        macro_rules! do_ret {
            () => {{
                let cs = ci.r().callstatus;
                if cs & (CIST_FRESH | CIST_MMRET) != 0 {
                    if cs & CIST_FRESH != 0 {
                        return Ok(None); // end this frame
                    }
                    // a metamethod run in place returned: finish the caller's instruction
                    ci = mm_return(st, ci)?;
                    continue 'frame;
                }
                match ci.previous() {
                    Some(prev) => {
                        ci = prev;
                        #[cfg(all(feature = "jit_check_d", not(feature = "jit_no_entry")))]
                        if !HOOK && ci.r().callstatus & CIST_JIT != 0 {
                            // (d) return into a frame whose function has compiled code
                            if crate::jit_hooks::jit_try_enter(st, ci) {
                                continue 'frame;
                            }
                        }
                        continue 'frame; // continue running caller in this frame
                    }
                    None => return Ok(None),
                }
            }};
        }
        // Test-only (feature `t15_oracle`): an instruction executed without a fetch (see `crate::oracle::unfetched`).
        macro_rules! note_unfetched {
            () => {{
                #[cfg(feature = "t15_oracle")]
                crate::oracle::note_unfetched();
            }};
        }
        #[cfg(not(rustynail_lua54))]
        macro_rules! do_tforloop {
            ($ii:expr) => {{
                let ii: Instruction = $ii;
                let ra = ii.a() as usize;
                if !rd!(ra + 3).is_nil() {
                    // continue loop?
                    pc_add!(-(ii.bx() as isize)); // jump back
                    hot_edge!();
                }
            }};
        }
        #[cfg(rustynail_lua54)]
        macro_rules! do_tforloop {
            ($ii:expr) => {{
                let ii: Instruction = $ii;
                let ra = ii.a() as usize;
                let v = rd!(ra + 4);
                if !v.is_nil() {
                    // continue loop?
                    wr!(ra + 2, v); // save control variable
                    pc_add!(-(ii.bx() as isize)); // jump back
                    hot_edge!();
                }
            }};
        }
        #[cfg(not(rustynail_lua54))]
        macro_rules! do_tforcall {
            ($ii:expr) => {{
                let ii: Instruction = $ii;
                let ra = ii.a() as usize;
                wr!(ra + 5, rd!(ra + 3)); // copy the control variable
                wr!(ra + 4, rd!(ra + 1)); // copy state
                wr!(ra + 3, rd!(ra)); // copy function
                st.set_top(sidx!(ra + 6));
                protect_nt!(st.call(sidx!(ra + 3), ii.c() as i32));
                note_unfetched!(); // the TFORLOOP below
                let ni = fetch_next!();
                pc_add!(1);
                do_tforloop!(ni);
            }};
        }
        // 5.4: 'ra' iterator, 'ra + 1' state, 'ra + 2' control variable, 'ra + 3' closing
        // variable; the call uses the stack from 'ra + 4'
        #[cfg(rustynail_lua54)]
        macro_rules! do_tforcall {
            ($ii:expr) => {{
                let ii: Instruction = $ii;
                let ra = ii.a() as usize;
                wr!(ra + 4, rd!(ra)); // push function, state, and control variable
                wr!(ra + 5, rd!(ra + 1));
                wr!(ra + 6, rd!(ra + 2));
                st.set_top(sidx!(ra + 7));
                protect_nt!(st.call(sidx!(ra + 4), ii.c() as i32));
                note_unfetched!(); // the TFORLOOP below
                let ni = fetch_next!();
                pc_add!(1);
                do_tforloop!(ni);
            }};
        }
        // Arithmetic on integers with a float fallback (`op_arith_aux`).
        macro_rules! modop {
            ($a:expr, $v1:expr, $v2:expr) => {{
                let (v1, v2): (Value, Value) = ($v1, $v2);
                if let (Some(x), Some(y)) = (v1.as_int(), v2.as_int()) {
                    match number::imod(x, y) {
                        Some(r) => {
                            pc_add!(1);
                            wr!($a, Value::int(r));
                        }
                        None => {
                            savestate!();
                            return Err(st.run_error(b"attempt to perform 'n%0'"));
                        }
                    }
                } else if let (Some(x), Some(y)) = (tonum(&v1), tonum(&v2)) {
                    pc_add!(1);
                    wr!($a, Value::float(number::fmod_lua(x, y)));
                }
            }};
        }
        macro_rules! idivop {
            ($a:expr, $v1:expr, $v2:expr) => {{
                let (v1, v2): (Value, Value) = ($v1, $v2);
                if let (Some(x), Some(y)) = (v1.as_int(), v2.as_int()) {
                    match number::idiv(x, y) {
                        Some(r) => {
                            pc_add!(1);
                            wr!($a, Value::int(r));
                        }
                        None => {
                            savestate!();
                            return Err(st.run_error(b"attempt to divide by zero"));
                        }
                    }
                } else if let (Some(x), Some(y)) = (tonum(&v1), tonum(&v2)) {
                    pc_add!(1);
                    wr!($a, Value::float(number::fidiv(x, y)));
                }
            }};
        }
        // Pointers to operands (register / constant) for the arithmetic fast paths, which read
        // tags and payloads separately (floats straight into float registers).
        macro_rules! rp {
            ($n:expr) => {{
                let n = $n;
                debug_assert_eq!(st.stack_gen(), gen, "stale base pointer");
                // SAFETY: VM2.
                unsafe { base.byte_add(Reg::off(n)).cast_const() }
            }};
        }
        macro_rules! kp {
            ($n:expr) => {{
                // SAFETY: VM3.
                unsafe { ci.r().kcache.add($n as usize) }
            }};
        }
        // SAFETY (both macros): VM2/VM3 — the operands point to initialised values; payloads are
        // read as the type their tag says.
        macro_rules! arith {
            ($a:expr, $p1:expr, $p2:expr, $iop:expr, $fop:expr) => {{
                let (p1, p2): (*const Value, *const Value) = ($p1, $p2);
                // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                let (t1, t2) = unsafe { (Value::load_tag(p1), Value::load_tag(p2)) };
                if t1 == LUA_VNUMINT && t2 == LUA_VNUMINT {
                    // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                    let (x, y) = unsafe { (Value::load_int_payload(p1), Value::load_int_payload(p2)) };
                    pc_add!(1);
                    wr!($a, Value::int($iop(x, y)));
                } else if t1 == LUA_VNUMFLT && t2 == LUA_VNUMFLT {
                    let (x, y) =
                        // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                        unsafe { (Value::load_float_payload(p1), Value::load_float_payload(p2)) };
                    pc_add!(1);
                    wr!($a, Value::float($fop(x, y)));
                } else if let (Some(x), Some(y)) =
                    // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                    unsafe { (tonum(&Value::load(p1)), tonum(&Value::load(p2))) }
                {
                    pc_add!(1);
                    wr!($a, Value::float($fop(x, y)));
                }
            }};
        }
        // Float-only arithmetic (`op_arithf_aux`).
        macro_rules! arithf {
            ($a:expr, $p1:expr, $p2:expr, $fop:expr) => {{
                let (p1, p2): (*const Value, *const Value) = ($p1, $p2);
                // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                let (t1, t2) = unsafe { (Value::load_tag(p1), Value::load_tag(p2)) };
                if t1 == LUA_VNUMFLT && t2 == LUA_VNUMFLT {
                    let (x, y) =
                        // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                        unsafe { (Value::load_float_payload(p1), Value::load_float_payload(p2)) };
                    pc_add!(1);
                    wr!($a, Value::float($fop(x, y)));
                } else if let (Some(x), Some(y)) =
                    // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                    unsafe { (tonum(&Value::load(p1)), tonum(&Value::load(p2))) }
                {
                    pc_add!(1);
                    wr!($a, Value::float($fop(x, y)));
                }
            }};
        }
        // Bitwise operations: two integers read straight from their slots (tags and payloads
        // separately, as `arith!`), anything else through the float-to-integer conversion.
        macro_rules! bitwisep {
            ($a:expr, $p1:expr, $p2:expr, $op:expr) => {{
                let (p1, p2): (*const Value, *const Value) = ($p1, $p2);
                // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                let (t1, t2) = unsafe { (Value::load_tag(p1), Value::load_tag(p2)) };
                if t1 == LUA_VNUMINT && t2 == LUA_VNUMINT {
                    // SAFETY: as above.
                    let x = unsafe { Value::load_int_payload(p1) };
                    // SAFETY: as above.
                    let y = unsafe { Value::load_int_payload(p2) };
                    pc_add!(1);
                    wr!($a, Value::int($op(x, y)));
                } else {
                    // SAFETY: as above.
                    let (v1, v2) = unsafe { (Value::load(p1), Value::load(p2)) };
                    bitwise!($a, v1, v2, $op);
                }
            }};
        }
        macro_rules! bitwise {
            ($a:expr, $v1:expr, $v2:expr, $op:expr) => {{
                let (v1, v2): (Value, Value) = ($v1, $v2);
                let x = match v1.as_int() {
                    Some(x) => Some(x),
                    None => to_integer_ns(&v1, F2I::Eq),
                };
                let y = match v2.as_int() {
                    Some(y) => Some(y),
                    None => to_integer_ns(&v2, F2I::Eq),
                };
                if let (Some(x), Some(y)) = (x, y) {
                    pc_add!(1);
                    wr!($a, Value::int($op(x, y)));
                }
            }};
        }
        macro_rules! order_i {
            ($i:expr, $a:expr, $opi:tt, $inv:expr, $tm:expr) => {{
                let va = rd!($a);
                let im = $i.sb();
                let cond = if let Some(x) = va.as_int() {
                    x $opi (im as i64)
                } else if let Some(f) = va.as_float() {
                    f $opi (im as f64)
                } else {
                    let isf = $i.c() != 0;
                    let aux = if isf { Value::float(im as f64) } else { Value::int(im as i64) };
                    let (p1, p2) = if $inv { (aux, va) } else { (va, aux) };
                    savepc!();
                    try_mm!(mm_bin(st, ci.r().top, p1, p2, $tm));
                    protect!(st.call_orderi_tm(va, im, $inv, isf, $tm))
                };
                docondjump!($i, cond);
            }};
        }
        // Run the metamethod frame `$e` (an `Option<Ci>`) in place if there is one. `pc` must
        // have been saved.
        macro_rules! try_mm {
            ($e:expr) => {{
                let r: Option<Ci> = $e;
                if let Some(nci) = r {
                    ci = nci;
                    continue 'frame;
                }
                // (`pc` was saved: re-reading it keeps it dead across the call, see `protect!`)
                pc = ci.r().savedpc;
                reload!();
            }};
        }
        // Table get with the result in register `$a`; `$t` is the container value (`Opnd`).
        macro_rules! slow_get {
            ($t:expr, $key:expr, $a:expr) => {{
                let t: Opnd = $t;
                let key: Value = $key;
                savepc!();
                try_mm!(mm_index(st, ci, t.v, key));
                protect!(st.finish_get(t, key, sidx!($a)));
            }};
        }

        // ---- main loop ----------------------------------------------------------------------
        loop {
            // P-O: a recording armed from this frame was aborted by a nested activation (the body of an inlined `pcall` raised
            // or left the recorded path): the abort cleared this frame's `trap`, nothing keeps it in the hooked copy
            #[cfg(feature = "jit_trace")]
            if HOOK && ci.r().trap == 0 && st.hook_mask() == 0 && !crate::jit_hooks::rec_active() {
                ci.w().savedpc = pc;
                return Ok(Some((ci, false)));
            }
            if HOOK && ci.r().trap != 0 {
                // J5 recorder (TR23.3/4): before the instruction of a recorded frame; `Leave` hands the frame
                // (`savedpc = pc`, `trap` fixed by the callee) to the fast copy.
                // With a hook installed the frame still needs `trace_call`/`trace_exec` (hooks and `trap` stay with
                // the hooked copy): fall through; `rec_step` left `trap = 1` for it.
                #[cfg(feature = "jit_trace")]
                if crate::jit_hooks::rec_active()
                    && crate::jit_hooks::rec_step(st, ci, pc) == crate::jit_hooks::RecStep::Leave
                    && st.hook_mask() == 0
                {
                    return Ok(Some((ci, false)));
                }
                // stack reallocation or hooks?
                st.trace_exec(pc)?; // handle hooks
                reload!(); // correct stack
            }
            // set by the fast interpreter's `updatetrap!` (only on paths that reach it, so
            // constant-false everywhere else)
            trapped = false;
            // Test-only per-instruction callback (feature `t15_oracle`, see `crate::oracle`).
            #[cfg(feature = "t15_oracle")]
            if crate::oracle::active() {
                let p = lcl!().proto();
                // SAFETY: VM1 — `pc` points into the proto's code array.
                let pcix = unsafe { pc.offset_from(p.code_ptr()) } as usize;
                crate::oracle::fire(
                    ci.as_ptr() as usize,
                    &*p as *const crate::func::Proto,
                    pcix,
                    st.slot_ptr(StkId(ci.r().func.0 + 1)),
                    usize::from(p.maxstacksize),
                );
            }
            // SAFETY: VM1.
            let i = Instruction(unsafe { *pc });
            // SAFETY: VM1 — one past an instruction is inside or at the end of the array.
            pc = unsafe { pc.add(1) };
            let raw = (i.0 & 0x7f) as u8;
            // SAFETY: `OpCode` is `repr(u8)` with every value `0..128` (the unused ones are the
            // `Reserved*` padding variants), and `raw` is a 7-bit value: no range check needed.
            let op: OpCode = unsafe { std::mem::transmute::<u8, OpCode>(raw) };
            let a = i.a() as usize;
            match op {
                OpCode::Move => {
                    wr!(a, rd!(i.b()));
                }
                OpCode::Loadi => wr!(a, Value::int(i.sbx() as i64)),
                OpCode::Loadf => wr!(a, Value::float(i.sbx() as f64)),
                OpCode::Loadk => wr!(a, kv!(i.bx())),
                OpCode::Loadkx => {
                    let ax = fetch_next!().ax();
                    pc_add!(1);
                    wr!(a, kv!(ax));
                }
                OpCode::Loadfalse => wr!(a, Value::FALSE),
                OpCode::Lfalseskip => {
                    wr!(a, Value::FALSE);
                    pc_add!(1); // skip next instruction
                }
                OpCode::Loadtrue => wr!(a, Value::TRUE),
                OpCode::Loadnil => {
                    for n in 0..=i.b() as usize {
                        wr!(a + n, Value::NIL);
                    }
                }
                OpCode::Getupval => {
                    // SAFETY: VM4.
                    let v = unsafe { Value::load((**lcl!().upvals_ptr().add(i.b() as usize)).v) };
                    wr!(a, v);
                }
                OpCode::Setupval => {
                    let v = rd!(a);
                    // SAFETY: VM4.
                    let uv = unsafe { *lcl!().upvals_ptr().add(i.b() as usize) };
                    // SAFETY: VM4 — `v` points to a live slot.
                    unsafe { Value::store((*uv).v, v) };
                    st.heap().barrier(uv.cast(), &v);
                }
                OpCode::Gettabup => {
                    // SAFETY: VM4.
                    let upval =
                        unsafe { Value::load((**lcl!().upvals_ptr().add(i.b() as usize)).v) };
                    let rc = kv!(i.c());
                    if upval.is_table() && rc.is_string() {
                        // SAFETY: both tags checked just above.
                        let (t, key) = unsafe { (upval.table_unchecked(), rc.strref_unchecked()) };
                        let v = t.get_short_str(key);
                        if !v.is_nil() {
                            wr!(a, v);
                            continue;
                        }
                        let v = index_chain_short_str(st, t, key);
                        // re-derive `base` (the stack did not move): keeps it dead across the
                        // call, otherwise LLVM spills it at every dispatch
                        reload!();
                        if v.tag() != LUA_VABSTKEY {
                            wr!(a, v);
                            continue;
                        }
                    }
                    slow_get!(Opnd::new(upval, OpLoc::Upvalue(i.b() as u8)), rc, a);
                }
                OpCode::Gettable => {
                    let rb = rd!(i.b());
                    let rc = rd!(i.c());
                    if rb.is_table() {
                        // SAFETY: tag checked just above.
                        let t = unsafe { rb.table_unchecked() };
                        if let Some(v) = rc.as_int().and_then(|n| t.get_int_arr(n)) {
                            wr!(a, v);
                            continue;
                        }
                        let v = match rc.as_int() {
                            Some(n) => t.get_int(n),
                            None => t.get_by_val(rc),
                        };
                        reload!(); // keeps `base` dead across the call (see index_chain_short_str)
                        if !v.is_nil() {
                            wr!(a, v);
                            continue;
                        }
                    }
                    slow_get!(Opnd::new(rb, OpLoc::Stack(sidx!(i.b()))), rc, a);
                }
                OpCode::Geti => {
                    let rb = rd!(i.b());
                    let c = i.c() as i64;
                    if rb.is_table() {
                        // SAFETY: tag checked just above.
                        let t = unsafe { rb.table_unchecked() };
                        if let Some(v) = t.get_int_arr(c) {
                            wr!(a, v);
                            continue;
                        }
                        let v = t.get_int(c);
                        if !v.is_nil() {
                            wr!(a, v);
                            continue;
                        }
                    }
                    slow_get!(Opnd::new(rb, OpLoc::Stack(sidx!(i.b()))), Value::int(c), a);
                }
                OpCode::Getfield => {
                    let rb = rd!(i.b());
                    let rc = kv!(i.c());
                    if rb.is_table() && rc.is_string() {
                        // SAFETY: both tags checked just above.
                        let (t, key) = unsafe { (rb.table_unchecked(), rc.strref_unchecked()) };
                        let v = t.get_short_str(key);
                        if !v.is_nil() {
                            wr!(a, v);
                            continue;
                        }
                        let v = index_chain_short_str(st, t, key);
                        // re-derive `base` (the stack did not move): keeps it dead across the
                        // call, otherwise LLVM spills it at every dispatch
                        reload!();
                        if v.tag() != LUA_VABSTKEY {
                            wr!(a, v);
                            continue;
                        }
                    }
                    slow_get!(Opnd::new(rb, OpLoc::Stack(sidx!(i.b()))), rc, a);
                }
                OpCode::Settabup => {
                    // SAFETY: VM4.
                    let upval = unsafe { Value::load((**lcl!().upvals_ptr().add(a)).v) };
                    let rb = kv!(i.b());
                    let rc = if i.k() != 0 { kv!(i.c()) } else { rd!(i.c()) };
                    let mut hres = -1;
                    if upval.is_table() && rb.is_string() {
                        // SAFETY: both tags checked just above.
                        let (t, key) = unsafe { (upval.table_unchecked(), rb.strref_unchecked()) };
                        // SAFETY: VM3 — `key` is a short string.
                        hres = unsafe { t.psetshortstr(key.as_ptr(), rc) };
                        reload!();
                        if hres == HOK {
                            st.heap().barrier_back(t.as_ptr().cast(), &rc);
                            continue;
                        }
                    }
                    if hres != HOK {
                        savepc!();
                        try_mm!(mm_newindex(st, ci, i, upval));
                    }
                    protect!(finish_set_op(st, ci, i, hres));
                }
                OpCode::Settable => {
                    let ra = rd!(a);
                    let rb = rd!(i.b()); // key (table is in 'ra')
                    let rc = if i.k() != 0 { kv!(i.c()) } else { rd!(i.c()) }; // value
                    let mut hres = -1;
                    if ra.is_table() {
                        // SAFETY: tag checked just above.
                        let t = unsafe { ra.table_unchecked() };
                        if let Some(n) = rb.as_int() {
                            if t.pset_existing_int_arr(n, rc) {
                                st.heap().barrier_back(t.as_ptr().cast(), &rc);
                                continue;
                            }
                        }
                        hres = match rb.as_int() {
                            Some(n) => t.psetint(n, rc),
                            None => t.pset(rb, rc),
                        };
                        reload!();
                        if hres == HOK {
                            st.heap().barrier_back(t.as_ptr().cast(), &rc);
                            continue;
                        }
                    }
                    if hres != HOK {
                        savepc!();
                        try_mm!(mm_newindex(st, ci, i, rd!(a)));
                    }
                    protect!(finish_set_op(st, ci, i, hres));
                }
                OpCode::Seti => {
                    let ra = rd!(a);
                    let b = i.b() as i64;
                    let rc = if i.k() != 0 { kv!(i.c()) } else { rd!(i.c()) };
                    let mut hres = -1;
                    if ra.is_table() {
                        // SAFETY: tag checked just above.
                        let t = unsafe { ra.table_unchecked() };
                        if t.pset_existing_int_arr(b, rc) {
                            st.heap().barrier_back(t.as_ptr().cast(), &rc);
                            continue;
                        }
                        hres = t.psetint(b, rc);
                        reload!();
                        if hres == HOK {
                            st.heap().barrier_back(t.as_ptr().cast(), &rc);
                            continue;
                        }
                    }
                    if hres != HOK {
                        savepc!();
                        try_mm!(mm_newindex(st, ci, i, rd!(a)));
                    }
                    protect!(finish_set_op(st, ci, i, hres));
                }
                OpCode::Setfield => {
                    let ra = rd!(a);
                    let rb = kv!(i.b());
                    let rc = if i.k() != 0 { kv!(i.c()) } else { rd!(i.c()) };
                    let mut hres = -1;
                    if ra.is_table() && rb.is_string() {
                        // SAFETY: both tags checked just above.
                        let (t, key) = unsafe { (ra.table_unchecked(), rb.strref_unchecked()) };
                        // SAFETY: VM3 — `key` is a short string.
                        let slot = match unsafe { t.pset_existing_shortstr_slot(key.as_ptr(), rc) }
                        {
                            Ok(()) => {
                                st.heap().barrier_back(t.as_ptr().cast(), &rc);
                                continue;
                            }
                            Err(slot) => slot,
                        };
                        // (continues from the lookup above: no second lookup of the key)
                        // SAFETY: VM3; `slot` is that lookup's result.
                        hres = unsafe { t.psetshortstr_after(key.as_ptr(), slot, rc) };
                        reload!();
                        if hres == HOK {
                            st.heap().barrier_back(t.as_ptr().cast(), &rc);
                            continue;
                        }
                    }
                    if hres != HOK {
                        savepc!();
                        try_mm!(mm_newindex(st, ci, i, rd!(a)));
                    }
                    protect!(finish_set_op(st, ci, i, hres));
                }
                OpCode::Newtable => {
                    #[cfg(not(rustynail_lua54))]
                    let (mut b, mut c, cmul) = (i.vb(), i.vc(), MAXARG_VC + 1);
                    #[cfg(rustynail_lua54)]
                    let (mut b, mut c, cmul) = (i.b(), i.c(), MAXARG_C + 1);
                    if b > 0 {
                        b = 1u32 << (b - 1); // hash size is 2^(b - 1)
                    }
                    if i.k() != 0 {
                        // non-zero extra argument: add it to array size
                        c = c.wrapping_add(fetch_next!().ax().wrapping_mul(cmul));
                    }
                    pc_add!(1); // skip extra argument
                    savepc!();
                    st.set_top(sidx!(a + 1)); // correct top in case of emergency GC
                    let t = st.new_table()?;
                    wr!(a, Value::table(t));
                    if b != 0 || c != 0 {
                        if let Err(e) = t.resize(st.heap(), c, b) {
                            return Err(st.table_error(e));
                        }
                    }
                    check_gc!(sidx!(a + 1));
                }
                OpCode::SelfOp => {
                    let rb = rd!(i.b());
                    #[cfg(not(rustynail_lua54))]
                    let rc = kv!(i.c());
                    // 5.4: `RKC` (a register or any string constant, possibly a long string: the
                    // short-string fast paths below test the variant)
                    #[cfg(rustynail_lua54)]
                    let rc = if i.k() != 0 { kv!(i.c()) } else { rd!(i.c()) };
                    #[cfg(rustynail_lua54)]
                    let rc_short = rc.tag() == ctb(LUA_VSHRSTR);
                    #[cfg(not(rustynail_lua54))]
                    let rc_short = rc.is_string();
                    wr!(a + 1, rb);
                    if rb.is_table() && rc_short {
                        // SAFETY: both tags checked just above.
                        let (t, key) = unsafe { (rb.table_unchecked(), rc.strref_unchecked()) };
                        let v = t.get_short_str(key);
                        if !v.is_nil() {
                            wr!(a, v);
                            continue;
                        }
                        let v = index_chain_short_str(st, t, key);
                        // re-derive `base` (the stack did not move): keeps it dead across the
                        // call, otherwise LLVM spills it at every dispatch
                        reload!();
                        if v.tag() != LUA_VABSTKEY {
                            wr!(a, v);
                            continue;
                        }
                    } else if rb.is_string() && rc_short {
                        // SAFETY: tag checked just above (VM3: a short string).
                        let v = index_string_short_str(st, unsafe { rc.strref_unchecked() });
                        reload!();
                        if v.tag() != LUA_VABSTKEY {
                            wr!(a, v);
                            continue;
                        }
                    }
                    #[cfg(rustynail_lua54)]
                    if !rc_short {
                        if let Some(t) = rb.as_table() {
                            // a long-string method name: the raw lookup `finish_get` expects
                            let v = t.get_by_val(rc);
                            reload!();
                            if !v.is_nil() {
                                wr!(a, v);
                                continue;
                            }
                        }
                    }
                    slow_get!(Opnd::new(rb, OpLoc::Stack(sidx!(i.b()))), rc, a);
                }
                OpCode::Addi => {
                    let v1 = rd!(i.b());
                    let imm = i.sc();
                    if let Some(x) = v1.as_int() {
                        pc_add!(1);
                        wr!(a, Value::int(x.wrapping_add(imm as i64)));
                    } else if let Some(f) = v1.as_float() {
                        pc_add!(1);
                        wr!(a, Value::float(f + imm as f64));
                    }
                }
                OpCode::Addk => arith!(a, rp!(i.b()), kp!(i.c()), i64::wrapping_add, |x, y| x + y),
                OpCode::Subk => arith!(a, rp!(i.b()), kp!(i.c()), i64::wrapping_sub, |x, y| x - y),
                OpCode::Mulk => arith!(a, rp!(i.b()), kp!(i.c()), i64::wrapping_mul, |x, y| x * y),
                OpCode::Modk => modop!(a, rd!(i.b()), kv!(i.c())),
                OpCode::Mod => modop!(a, rd!(i.b()), rd!(i.c())),
                OpCode::Powk => arithf!(a, rp!(i.b()), kp!(i.c()), number::pow_lua),
                OpCode::Divk => arithf!(a, rp!(i.b()), kp!(i.c()), |x, y| x / y),
                OpCode::Idivk => idivop!(a, rd!(i.b()), kv!(i.c())),
                OpCode::Idiv => idivop!(a, rd!(i.b()), rd!(i.c())),
                OpCode::Bandk => bitwisep!(a, rp!(i.b()), kp!(i.c()), |x, y| x & y),
                OpCode::Bork => bitwisep!(a, rp!(i.b()), kp!(i.c()), |x, y| x | y),
                OpCode::Bxork => bitwisep!(a, rp!(i.b()), kp!(i.c()), |x, y| x ^ y),
                OpCode::Shli => {
                    let rb = rd!(i.b());
                    let ic = i.sc() as i64;
                    if let Some(ib) = rb.as_int().or_else(|| to_integer_ns(&rb, F2I::Eq)) {
                        pc_add!(1);
                        wr!(a, Value::int(number::shiftl(ic, ib)));
                    }
                }
                OpCode::Shri => {
                    let rb = rd!(i.b());
                    let ic = i.sc() as i64;
                    if let Some(ib) = rb.as_int().or_else(|| to_integer_ns(&rb, F2I::Eq)) {
                        pc_add!(1);
                        wr!(a, Value::int(number::shiftl(ib, -ic)));
                    }
                }
                OpCode::Add => arith!(a, rp!(i.b()), rp!(i.c()), i64::wrapping_add, |x, y| x + y),
                OpCode::Sub => arith!(a, rp!(i.b()), rp!(i.c()), i64::wrapping_sub, |x, y| x - y),
                OpCode::Mul => arith!(a, rp!(i.b()), rp!(i.c()), i64::wrapping_mul, |x, y| x * y),
                OpCode::Pow => arithf!(a, rp!(i.b()), rp!(i.c()), number::pow_lua),
                OpCode::Div => arithf!(a, rp!(i.b()), rp!(i.c()), |x, y| x / y),
                OpCode::Band => bitwisep!(a, rp!(i.b()), rp!(i.c()), |x, y| x & y),
                OpCode::Bor => bitwisep!(a, rp!(i.b()), rp!(i.c()), |x, y| x | y),
                OpCode::Bxor => bitwisep!(a, rp!(i.b()), rp!(i.c()), |x, y| x ^ y),
                OpCode::Shl => bitwisep!(a, rp!(i.b()), rp!(i.c()), number::shiftl),
                OpCode::Shr => bitwisep!(a, rp!(i.b()), rp!(i.c()), number::shiftr),
                OpCode::Mmbin => {
                    // SAFETY: VM1 — the arithmetic instruction precedes this one.
                    let pi = Instruction(unsafe { *pc.sub(2) });
                    let result = sidx!(pi.a());
                    let tm = Tm::from_u8(i.c() as u8);
                    savepc!();
                    try_mm!(mm_bin(st, ci.r().top, rd!(a), rd!(i.b()), tm));
                    let p1 = Opnd::new(rd!(a), OpLoc::Stack(sidx!(a)));
                    let p2 = Opnd::new(rd!(i.b()), OpLoc::Stack(sidx!(i.b())));
                    protect!(st.try_bin_tm(p1, p2, result, tm));
                }
                OpCode::Mmbini => {
                    // SAFETY: VM1 — the arithmetic instruction precedes this one.
                    let pi = Instruction(unsafe { *pc.sub(2) });
                    let result = sidx!(pi.a());
                    let tm = Tm::from_u8(i.c() as u8);
                    let (v1, v2) = (rd!(a), Value::int(i.sb() as i64));
                    let (q1, q2) = if i.k() != 0 { (v2, v1) } else { (v1, v2) };
                    savepc!();
                    try_mm!(mm_bin(st, ci.r().top, q1, q2, tm));
                    let p1 = Opnd::new(rd!(a), OpLoc::Stack(sidx!(a)));
                    protect!(st.try_bini_tm(p1, i.sb() as i64, i.k() != 0, result, tm));
                }
                OpCode::Mmbink => {
                    // SAFETY: VM1 — the arithmetic instruction precedes this one.
                    let pi = Instruction(unsafe { *pc.sub(2) });
                    let result = sidx!(pi.a());
                    let tm = Tm::from_u8(i.c() as u8);
                    let (v1, v2) = (rd!(a), kv!(i.b()));
                    let (q1, q2) = if i.k() != 0 { (v2, v1) } else { (v1, v2) };
                    savepc!();
                    try_mm!(mm_bin(st, ci.r().top, q1, q2, tm));
                    let p1 = Opnd::new(rd!(a), OpLoc::Stack(sidx!(a)));
                    let p2 = Opnd::new(kv!(i.b()), OpLoc::Constant(i.b()));
                    protect!(st.try_bin_assoc_tm(p1, p2, i.k() != 0, result, tm));
                }
                OpCode::Unm => {
                    let rb = rd!(i.b());
                    if let Some(x) = rb.as_int() {
                        wr!(a, Value::int(x.wrapping_neg()));
                    } else if let Some(f) = tonum(&rb) {
                        wr!(a, Value::float(-f));
                    } else {
                        savepc!();
                        try_mm!(mm_bin(st, ci.r().top, rb, rb, Tm::Unm));
                        let rb = rd!(i.b());
                        let o = Opnd::new(rb, OpLoc::Stack(sidx!(i.b())));
                        protect!(st.try_bin_tm(o, o, sidx!(a), Tm::Unm));
                    }
                }
                OpCode::Bnot => {
                    let rb = rd!(i.b());
                    if let Some(x) = to_integer_ns(&rb, F2I::Eq) {
                        wr!(a, Value::int(!x));
                    } else {
                        savepc!();
                        try_mm!(mm_bin(st, ci.r().top, rb, rb, Tm::BNot));
                        let rb = rd!(i.b());
                        let o = Opnd::new(rb, OpLoc::Stack(sidx!(i.b())));
                        protect!(st.try_bin_tm(o, o, sidx!(a), Tm::BNot));
                    }
                }
                OpCode::Not => {
                    let rb = rd!(i.b());
                    wr!(a, Value::boolean(rb.is_falsy()));
                }
                OpCode::Len => {
                    let rb = rd!(i.b());
                    if rb.is_table() {
                        // SAFETY: tag checked just above.
                        let t = unsafe { rb.table_unchecked() };
                        if TableRef::no_tm(t.metatable(), Tm::Len) {
                            let n = t.len(st.heap());
                            reload!();
                            wr!(a, Value::int(n as i64));
                            continue;
                        }
                    }
                    if rb.is_table() {
                        savepc!();
                        try_mm!(mm_bin(st, ci.r().top, rb, rb, Tm::Len));
                    }
                    let rb = rd!(i.b());
                    protect!(st.obj_len(sidx!(a), Opnd::new(rb, OpLoc::Stack(sidx!(i.b())))));
                }
                OpCode::Concat => {
                    let n = i.b() as usize; // number of elements to concatenate
                    st.set_top(sidx!(a + n)); // mark the end of concat operands
                    if n == 2 && !rd!(a).is_string() {
                        // `__concat` of two operands, the first one not a string: unless both are
                        // numbers or strings, `luaV_concat` would call `luaT_tryconcatTM` with
                        // them unchanged (a number first operand just finds no metamethod)
                        let (v1, v2) = (rd!(a), rd!(a + 1));
                        if !(v1.is_number() && (v2.is_string() || v2.is_number())) {
                            savepc!();
                            try_mm!(mm_bin(st, sidx!(a + 2), v1, v2, Tm::Concat));
                        }
                    }
                    protect_nt!(st.concat(n));
                    let t = st.top();
                    check_gc!(t); // 'concat' ensures correct top
                }
                OpCode::Close => {
                    protect!(st.close(sidx!(a), Status::Ok as u8, true));
                }
                OpCode::Tbc => {
                    // create new to-be-closed upvalue
                    half_protect!(st.new_tbc_upval(sidx!(a)));
                }
                OpCode::Jmp => dojump!(i.sj(), 0),
                OpCode::Eq => {
                    let (va, vb) = (rd!(a), rd!(i.b()));
                    let cond = if va.raw_equal(&vb) {
                        true
                    } else if va.tag() == vb.tag() && (va.is_table() || va.is_userdata()) {
                        savepc!();
                        try_mm!(mm_eq(st, ci, va, vb));
                        protect!(st.equal_obj(rd!(a), rd!(i.b())))
                    } else {
                        false
                    };
                    docondjump!(i, cond);
                }
                OpCode::Lt => {
                    let (pa, pb) = (rp!(a), rp!(i.b()));
                    // SAFETY: VM2 — payloads are read as the type their tag says.
                    let (ta, tb) = unsafe { (Value::load_tag(pa), Value::load_tag(pb)) };
                    let cond = if ta == LUA_VNUMINT && tb == LUA_VNUMINT {
                        // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                        unsafe { Value::load_int_payload(pa) < Value::load_int_payload(pb) }
                    } else if ta == LUA_VNUMFLT && tb == LUA_VNUMFLT {
                        // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                        unsafe { Value::load_float_payload(pa) < Value::load_float_payload(pb) }
                    } else if let (Some(x), Some(y)) = (rd!(a).as_number(), rd!(i.b()).as_number())
                    {
                        number::lt_num(x, y)
                    } else {
                        if !(rd!(a).is_string() && rd!(i.b()).is_string()) {
                            savepc!();
                            try_mm!(mm_bin(st, ci.r().top, rd!(a), rd!(i.b()), Tm::Lt));
                        }
                        protect!(st.less_than(rd!(a), rd!(i.b())))
                    };
                    docondjump!(i, cond);
                }
                OpCode::Le => {
                    let (pa, pb) = (rp!(a), rp!(i.b()));
                    // SAFETY: VM2 — payloads are read as the type their tag says.
                    let (ta, tb) = unsafe { (Value::load_tag(pa), Value::load_tag(pb)) };
                    let cond = if ta == LUA_VNUMINT && tb == LUA_VNUMINT {
                        // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                        unsafe { Value::load_int_payload(pa) <= Value::load_int_payload(pb) }
                    } else if ta == LUA_VNUMFLT && tb == LUA_VNUMFLT {
                        // SAFETY: VM2/VM3 — initialised operands; payload read as its tag says.
                        unsafe { Value::load_float_payload(pa) <= Value::load_float_payload(pb) }
                    } else if let (Some(x), Some(y)) = (rd!(a).as_number(), rd!(i.b()).as_number())
                    {
                        number::le_num(x, y)
                    } else {
                        if !(rd!(a).is_string() && rd!(i.b()).is_string()) {
                            savepc!();
                            try_mm!(mm_bin(st, ci.r().top, rd!(a), rd!(i.b()), Tm::Le));
                        }
                        protect!(st.less_equal(rd!(a), rd!(i.b())))
                    };
                    docondjump!(i, cond);
                }
                OpCode::Eqk => {
                    // basic types do not use '__eq'; we can use raw equality
                    let cond = rd!(a).raw_equal(&kv!(i.b()));
                    docondjump!(i, cond);
                }
                OpCode::Eqi => {
                    let va = rd!(a);
                    let im = i.sb();
                    let cond = if let Some(x) = va.as_int() {
                        x == im as i64
                    } else if let Some(f) = va.as_float() {
                        f == im as f64
                    } else {
                        false // other types cannot be equal to a number
                    };
                    docondjump!(i, cond);
                }
                OpCode::Lti => order_i!(i, a, <, false, Tm::Lt),
                OpCode::Lei => order_i!(i, a, <=, false, Tm::Le),
                OpCode::Gti => order_i!(i, a, >, true, Tm::Lt),
                OpCode::Gei => order_i!(i, a, >=, true, Tm::Le),
                OpCode::Test => {
                    let cond = !rd!(a).is_falsy();
                    docondjump!(i, cond);
                }
                OpCode::Testset => {
                    let rb = rd!(i.b());
                    if rb.is_falsy() == (i.k() != 0) {
                        pc_add!(1);
                    } else {
                        wr!(a, rb);
                        donextjump!();
                    }
                }
                OpCode::Call => {
                    let b = i.b() as usize;
                    let nresults = i.c() as i32 - 1;
                    if b != 0 {
                        // fixed number of arguments?
                        st.set_top(sidx!(a + b)); // top signals number of arguments
                    } // else previous instruction set top
                    savepc!(); // in case of errors
                    let fast = st.pre_call_lua_fast(sidx!(a), nresults);
                    let res = match fast {
                        Some(c) => Some(c),
                        None => {
                            if st.pre_call_native_fast(sidx!(a), nresults)? {
                                None // native function done (results in place)
                            } else {
                                st.pre_call(sidx!(a), nresults)?
                            }
                        }
                    };
                    match res {
                        Some(newci) => {
                            // Lua call: run function in this same Rust frame
                            ci = newci;
                            if st.hook_mask() != 0 {
                                ci.w().trap = 1;
                            }
                            continue 'frame;
                        }
                        None => {
                            // native call; nothing else to be done
                            reload!();
                            updatetrap!();
                        }
                    }
                }
                OpCode::Tailcall => {
                    let mut b = i.b() as usize; // number of arguments + 1 (function)
                    let nparams1 = i.c() as usize;
                    // delta is virtual 'func' - real 'func' (vararg functions)
                    let delta = if nparams1 != 0 {
                        ci.r().nextraargs as usize + nparams1
                    } else {
                        0
                    };
                    if b != 0 {
                        st.set_top(sidx!(a + b));
                    } else {
                        // previous instruction set top
                        b = st.top() - sidx!(a);
                    }
                    savepc!(); // several calls here can raise errors
                    if i.k() != 0 {
                        st.close_upvals(sidx!(0usize)); // close upvalues from current call
                    }
                    let n = st.pre_tail_call(ci, sidx!(a), b, delta)?;
                    if n < 0 {
                        // Lua function: execute the callee
                        if st.hook_mask() != 0 {
                            ci.w().trap = 1;
                        }
                        continue 'frame;
                    }
                    // C function: finish the caller
                    ci.w().func -= delta; // restore 'func' (if vararg)
                    st.post_call(ci, n as usize)?;
                    updatetrap!(); // 'post_call' can change hooks
                    do_ret!();
                }
                OpCode::Return => {
                    let mut n = i.b() as i32 - 1; // number of results
                    let nparams1 = i.c() as usize;
                    if n < 0 {
                        // not fixed?
                        n = (st.top() - sidx!(a)) as i32; // get what is available
                    }
                    savepc!();
                    if i.k() != 0 {
                        // may there be open upvalues?
                        ci.w().u2 = n; // save number of returns
                        let t = ci.r().top;
                        if st.top() < t {
                            st.set_top(t);
                        }
                        st.close(sidx!(0usize), CLOSEKTOP, true)?;
                        reload!();
                    }
                    // (the registers are addressed relative to the current `func`: compute the new
                    // top before the vararg adjustment moves it back)
                    let new_top = sidx!(a + n as usize);
                    if nparams1 != 0 {
                        // vararg function?
                        ci.w().func -= ci.r().nextraargs as usize + nparams1;
                    }
                    st.set_top(new_top);
                    st.post_call(ci, n as usize)?;
                    updatetrap!(); // 'post_call' can change hooks
                    do_ret!();
                }
                OpCode::Return0 => {
                    if st.hook_mask() != 0 {
                        st.set_top(sidx!(a));
                        savepc!();
                        st.post_call(ci, 0)?;
                        ci.w().trap = 1;
                    } else {
                        // do the 'post_call' here
                        let mut nres = ci.r().nresults();
                        if let Some(prev) = ci.previous() {
                            st.set_ci(prev); // back to caller
                        }
                        st.set_top(ci.r().func);
                        while nres > 0 {
                            let t = st.top();
                            st.set(t, Value::NIL); // all results are nil
                            st.set_top(t + 1);
                            nres -= 1;
                        }
                    }
                    do_ret!();
                }
                OpCode::Return1 => {
                    if st.hook_mask() != 0 {
                        st.set_top(sidx!(a + 1));
                        savepc!();
                        st.post_call(ci, 1)?;
                        ci.w().trap = 1;
                    } else {
                        // do the 'post_call' here
                        let mut nres = ci.r().nresults();
                        if let Some(prev) = ci.previous() {
                            st.set_ci(prev); // back to caller
                        }
                        if nres == 0 {
                            st.set_top(ci.r().func); // asked for no results
                        } else {
                            let v = rd!(a);
                            // the function slot is `base - 1` (VM2): no stack-pointer reload
                            // SAFETY: VM2.
                            unsafe { Value::store(base.sub(1), v) }; // at least this result
                            st.set_top(ci.r().func + 1);
                            while nres > 1 {
                                let t = st.top();
                                st.set(t, Value::NIL); // complete missing results
                                st.set_top(t + 1);
                                nres -= 1;
                            }
                        }
                    }
                    do_ret!();
                }
                #[cfg(rustynail_lua54)]
                OpCode::Forloop => {
                    // 5.4: 'ra' internal index, 'ra + 1' counter or limit, 'ra + 2' step,
                    // 'ra + 3' control variable
                    if rd!(a + 2).tag() == LUA_VNUMINT {
                        // SAFETY: VM2 — `forprep` left the loop values as integers.
                        let more = unsafe {
                            let count =
                                Value::load_int_payload(base.byte_add(Reg::off(a + 1))) as u64;
                            if count > 0 {
                                let step = Value::load_int_payload(base.byte_add(Reg::off(a + 2)));
                                let idx = Value::load_int_payload(base.byte_add(Reg::off(a)));
                                Value::store_int_payload(
                                    base.byte_add(Reg::off(a + 1)),
                                    (count - 1) as i64,
                                );
                                let idx = idx.wrapping_add(step);
                                Value::store_int_payload(base.byte_add(Reg::off(a)), idx);
                                Value::store(base.byte_add(Reg::off(a + 3)), Value::int(idx));
                            }
                            count > 0
                        };
                        if more {
                            pc_add!(-(i.bx() as isize)); // jump back
                        }
                    } else {
                        // SAFETY: VM2 — `forprep` left the loop values as floats.
                        let (step, limit, idx) = unsafe {
                            (
                                Value::load_float_payload(base.byte_add(Reg::off(a + 2))),
                                Value::load_float_payload(base.byte_add(Reg::off(a + 1))),
                                Value::load_float_payload(base.byte_add(Reg::off(a))),
                            )
                        };
                        let idx = idx + step;
                        if if 0.0 < step {
                            idx <= limit
                        } else {
                            limit <= idx
                        } {
                            wr!(a, Value::float(idx)); // update internal index
                            wr!(a + 3, Value::float(idx)); // and control variable
                            pc_add!(-(i.bx() as isize)); // jump back
                        }
                    }
                    hot_edge!();
                    updatetrap!(); // allows a signal to break the loop
                }
                #[cfg(not(rustynail_lua54))]
                OpCode::Forloop => {
                    let step_v = rd!(a + 1);
                    if step_v.tag() == LUA_VNUMINT {
                        // integer loop: `forprep` left the counter, step and control variable as
                        // integers, so only payloads are touched
                        // SAFETY: VM2 — registers of this frame, all integers (see above).
                        let more = unsafe {
                            let step = Value::load_int_payload(base.byte_add(Reg::off(a + 1)));
                            let count = Value::load_int_payload(base.byte_add(Reg::off(a))) as u64;
                            if count > 0 {
                                // still more iterations?
                                let idx = Value::load_int_payload(base.byte_add(Reg::off(a + 2)));
                                Value::store_int_payload(
                                    base.byte_add(Reg::off(a)),
                                    (count - 1) as i64,
                                );
                                Value::store_int_payload(
                                    base.byte_add(Reg::off(a + 2)),
                                    idx.wrapping_add(step),
                                );
                            }
                            count > 0
                        };
                        if more {
                            pc_add!(-(i.bx() as isize)); // jump back
                        }
                    } else {
                        // float loop
                        // SAFETY: VM2 — `forprep` left the three loop values as floats.
                        let (step, limit, idx) = unsafe {
                            (
                                Value::load_float_payload(base.byte_add(Reg::off(a + 1))),
                                Value::load_float_payload(base.byte_add(Reg::off(a))),
                                Value::load_float_payload(base.byte_add(Reg::off(a + 2))),
                            )
                        };
                        let idx = idx + step;
                        if if 0.0 < step {
                            idx <= limit
                        } else {
                            limit <= idx
                        } {
                            wr!(a + 2, Value::float(idx)); // update control variable
                            pc_add!(-(i.bx() as isize)); // jump back
                        }
                    }
                    hot_edge!();
                    updatetrap!(); // allows a signal to break the loop
                }
                OpCode::Forprep => {
                    savestate!(); // in case of errors
                    if st.for_prep(sidx!(a))? {
                        pc_add!(i.bx() as isize + 1); // skip the loop
                    }
                }
                #[cfg(rustynail_lua54)]
                OpCode::Tforprep => {
                    // create to-be-closed upvalue (if needed)
                    half_protect!(st.new_tbc_upval(sidx!(a + 3)));
                    pc_add!(i.bx() as isize);
                    let ni = fetch_next!(); // go to next instruction (TFORCALL)
                    pc_add!(1);
                    note_unfetched!(); // the TFORCALL
                    do_tforcall!(ni);
                }
                #[cfg(not(rustynail_lua54))]
                OpCode::Tforprep => {
                    // swap the control and closing variables; mark the closing one as tbc
                    let temp = rd!(a + 3);
                    wr!(a + 3, rd!(a + 2));
                    wr!(a + 2, temp);
                    half_protect!(st.new_tbc_upval(sidx!(a + 2)));
                    pc_add!(i.bx() as isize); // go to end of the loop
                    let ni = fetch_next!(); // fetch next instruction (TFORCALL)
                    pc_add!(1);
                    note_unfetched!(); // the TFORCALL
                    do_tforcall!(ni);
                }
                OpCode::Tforcall => do_tforcall!(i),
                OpCode::Tforloop => do_tforloop!(i),
                OpCode::Setlist => {
                    #[cfg(not(rustynail_lua54))]
                    let (mut n, mut last, cmul) =
                        (i.vb() as usize, i.vc() as usize, MAXARG_VC as usize + 1);
                    #[cfg(rustynail_lua54)]
                    let (mut n, mut last, cmul) =
                        (i.b() as usize, i.c() as usize, MAXARG_C as usize + 1);
                    let Some(h) = rd!(a).as_table() else {
                        savestate!();
                        return Err(st.run_error(b"internal error: SETLIST on a non-table"));
                    };
                    if n == 0 {
                        n = (st.top() - sidx!(a)) - 1; // get up to the top
                    } else {
                        let t = ci.r().top;
                        st.set_top(t); // correct top in case of emergency GC
                    }
                    last += n;
                    if i.k() != 0 {
                        last += fetch_next!().ax() as usize * cmul;
                        pc_add!(1);
                    }
                    // when 'n' is known, table should have proper size
                    if last > h.array_size() as usize {
                        savepc!();
                        if let Err(e) = h.resize_array(st.heap(), last as u32) {
                            return Err(st.table_error(e));
                        }
                    }
                    while n > 0 {
                        let val = rd!(a + n);
                        if let Err(e) = h.set_int(st.heap(), last as i64, &val) {
                            savepc!();
                            return Err(st.table_error(e));
                        }
                        last -= 1;
                        st.heap().barrier_back(h.as_ptr().cast(), &val);
                        n -= 1;
                    }
                }
                OpCode::Closure => {
                    let sub = lcl!().proto().protos()[i.bx() as usize];
                    half_protect!(push_closure(st, lcl!(), sub, ci.r().func.0 + 1, sidx!(a)));
                    check_gc!(sidx!(a + 1));
                }
                OpCode::Vararg => {
                    let n = i.c() as i32 - 1; // required results (-1 means all)
                    #[cfg(not(rustynail_lua54))]
                    let vatab = if i.k() != 0 { i.b() as i32 } else { -1 };
                    #[cfg(rustynail_lua54)]
                    let vatab = -1;
                    protect!(st.get_varargs(ci, sidx!(a), n, vatab));
                }
                #[cfg(not(rustynail_lua54))]
                OpCode::Getvarg => {
                    let rc = rd!(i.c());
                    st.get_vararg(ci, sidx!(a), &rc);
                }
                #[cfg(not(rustynail_lua54))]
                OpCode::Errnnil => {
                    if !rd!(a).is_nil() {
                        savestate!();
                        let msg =
                            crate::debuginfo::errnnil_message(&*lcl!().proto(), i.bx() as usize);
                        return Err(st.run_error(&msg));
                    }
                }
                OpCode::Varargprep => {
                    protect_nt!(st.adjust_varargs(ci, lcl!().proto()));
                    if st.hook_mask() != 0 {
                        // hooks start here (see `trace_call`); previous "protect" updated trap
                        ci.w().trap = 1;
                        st.hook_call(ci)?;
                        st.tm().oldpc = 1; // next opcode will be seen as a "new" line
                    }
                    reload!();
                }
                OpCode::Extraarg => {}
                _ => {
                    // SAFETY: VM1 — `Reserved*` opcodes never reach the interpreter: the compiler
                    // does not emit them and the undumper rejects them. (This arm lets the
                    // dispatch be a bare 128-entry jump table.)
                    unsafe { std::hint::unreachable_unchecked() }
                }
            }
            if !HOOK && trapped {
                savepc!();
                return Ok(Some((ci, true)));
            }
        }
    }
}

/// `pushclosure`: create a new Lua closure for `sub`, put it in slot `ra` and initialise its
/// upvalues.
pub(crate) fn push_closure(
    st: &mut State,
    cl: Gc<LClosure>,
    sub: Gc<crate::func::Proto>,
    base_idx: usize,
    ra: StkId,
) -> LuaResult<()> {
    let nup = sub.upvalue_descs().len();
    let ncl = st.try_alloc(|h| h.new_lclosure(nup))?;
    // SAFETY: `ncl` was created with `sub.upvalue_descs().len()` upvalue slots.
    unsafe { ncl.set_proto(sub) };
    st.set(ra, Value::lclosure(ncl)); // anchor new closure in stack
    for n in 0..nup {
        let d = sub.upvalue_descs()[n];
        let uv = if d.instack {
            // upvalue refers to local variable
            st.find_upval(StkId(base_idx + d.idx as usize))?
        } else {
            cl.upval(d.idx as usize) // get upvalue from enclosing function
        };
        ncl.set_upval(st.heap(), n, uv);
    }
    Ok(())
}

#[allow(dead_code)]
fn _unused(_: Number, _: f64) {
    let _ = to_float;
}

/// `luaV_finishOp`: finish the instruction of the current Lua frame that was interrupted by a
/// yield inside a metamethod (or a yielded `__close`), using the value(s) left on the stack.
pub(crate) fn finish_op(st: &mut State) -> LuaResult<()> {
    let ci = st.ci();
    let base_idx = ci.r().func.0 + 1;
    // SAFETY: VM1 — the frame was interrupted after fetching an instruction, so
    // `savedpc - 1` is inside the code array.
    let inst = Instruction(unsafe { *ci.r().savedpc.sub(1) });
    let Some(op) = inst.opcode() else {
        return Ok(());
    };
    match op {
        OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink => {
            // SAFETY: an MMBIN* instruction is always preceded by its arithmetic instruction.
            let prev = Instruction(unsafe { *ci.r().savedpc.sub(2) });
            let t = st.top() - 1;
            let v = st.get(t);
            st.set(StkId(base_idx + prev.a() as usize), v);
            st.set_top(t);
        }
        OpCode::Unm
        | OpCode::Bnot
        | OpCode::Len
        | OpCode::Gettabup
        | OpCode::Gettable
        | OpCode::Geti
        | OpCode::Getfield
        | OpCode::SelfOp => {
            let t = st.top() - 1;
            let v = st.get(t);
            st.set(StkId(base_idx + inst.a() as usize), v);
            st.set_top(t);
        }
        OpCode::Lt
        | OpCode::Le
        | OpCode::Lti
        | OpCode::Lei
        | OpCode::Gti
        | OpCode::Gei
        | OpCode::Eq => {
            let t = st.top() - 1;
            #[allow(unused_mut)]
            let mut res = !st.get(t).is_falsy();
            st.set_top(t);
            #[cfg(rustynail_lua54)]
            if ci.r().callstatus & CIST_LEQ != 0 {
                // "<=" using "<" instead?
                ci.w().callstatus ^= CIST_LEQ; // clear mark
                res = !res; // negate result
            }
            if (res as u32) != inst.k() {
                // condition failed: skip the jump instruction
                // SAFETY: VM1 — the comparison is followed by a JMP.
                ci.w().savedpc = unsafe { ci.r().savedpc.add(1) };
            }
        }
        OpCode::Concat => {
            let top = st.top() - 1; // top when `try_concat_tm` was called
            let a = base_idx + inst.a() as usize;
            let total = top.0 - 1 - a; // yet to concatenate
            let v = st.get(top);
            st.set(top - 2, v); // put TM result in proper position
            st.set_top(top - 1); // top is one after last element (at top-2)
            st.concat(total)?; // concat them (may yield again)
        }
        OpCode::Close => {
            // yielded closing variables: repeat the instruction to close the others
            // SAFETY: undo the fetch increment.
            ci.w().savedpc = unsafe { ci.r().savedpc.sub(1) };
        }
        OpCode::Return => {
            let ra = StkId(base_idx + inst.a() as usize);
            // adjust top to signal the right number of returns (in case of 'up to top')
            st.set_top(ra + ci.r().u2 as usize);
            // repeat the instruction to close the other variables and complete the return
            // SAFETY: undo the fetch increment.
            ci.w().savedpc = unsafe { ci.r().savedpc.sub(1) };
        }
        _ => {} // CALL, TAILCALL, TFORCALL, SET*: nothing to finish
    }
    Ok(())
}
