//! `extern "C"` helpers called by compiled code (docs/JIT_DESIGN.md section 4). They mirror the
//! interpreter's `OP_CALL` / `OP_TAILCALL` slow paths exactly (same `pre_call*` functions, same order of
//! effects), so everything that is not an inline fast path of the compiled code behaves like the
//! interpreter. Compiled code has set `ci.savedpc` (the instruction after the call) and `top` (the end of
//! the arguments) before calling, like `savepc!()` / `st.set_top(..)` in the interpreter.
//!
//! Return value (`eax`): [`H_DONE`], [`H_LUA`], [`H_ERR`] or [`H_RETURNED`]. On [`H_ERR`] the status is in
//! the state's `JitSlot` ([`crate::jit_hooks::set_error`]). Every helper is `extern "C"`: a panic in it
//! aborts instead of unwinding through generated code (invariant P1). After any helper the stack may have
//! moved: compiled code reloads its frame base.

use crate::arith::MAXTAGLOOP;
use crate::instruction::Instruction;
use crate::jit_hooks::{set_error, stress_move_stack, stress_stackmove};
use crate::meta::Tm;
use crate::state::{CallInfo, Ci, State, StkId, Thread};
use crate::status::LuaResult;
use crate::table::{TableRef, HOK};
use crate::value::Value;
use crate::ver::active::OpCode;
use crate::vm_unsafe::{finish_set_op, push_closure, tonum};

/// A native function was called and finished; its results are in place.
pub const H_DONE: u32 = 0;
/// A Lua frame was pushed (call) or rewritten in place (tail call); `thread.ci` designates it.
pub const H_LUA: u32 = 1;
/// An error occurred (status recorded with `set_error`).
pub const H_ERR: u32 = 2;
/// A tail call to a native function finished and popped the frame (the caller returns like `RETURN`).
pub const H_RETURNED: u32 = 3;
/// J2a helpers only: the helper did nothing observable and the instruction needs the interpreter
/// (a metamethod function, an error, an unusual operand): compiled code leaves at the instruction.
pub const H_EXIT: u32 = 4;
/// C2: [`jit_cmp`] only: the comparison is true / false (no side effect, no error, no metamethod).
pub const H_TRUE: u32 = 5;
/// See [`H_TRUE`].
pub const H_FALSE: u32 = 6;
/// BUNDLE-V: [`jit_tailcall`] only: a native tail callee finished in a metamethod frame; the frame was popped like [`jit_mm_ret`] does (`thread.ci` = the caller).
pub const H_MMPOP: u32 = 7;
/// BUNDLE-V: [`jit_tailcall`] only: as [`H_MMPOP`], but the caller's instruction was finished by the interpreter's `mm_return` (compiled code leaves with `FrameChanged`).
pub const H_MMFC: u32 = 8;

fn finish(st: &mut State, r: LuaResult<u32>) -> u32 {
    if stress_stackmove() {
        stress_move_stack(st);
    }
    match r {
        Ok(code) => code,
        Err(s) => {
            set_error(st, s);
            H_ERR
        }
    }
}

/// `OP_CALL` (after the interpreter's `st.set_top(..)` / `savepc!()`): `instr` is the CALL instruction.
///
/// # Safety
/// Called only from compiled code with the running thread and its current `CallInfo` (a Lua frame).
pub unsafe extern "C" fn jit_call(th: *mut Thread, ci: *mut CallInfo, instr: u64, _x: u64) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`; `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let func = StkId(ci.r().func.0 + 1 + i.a() as usize);
    let nresults = i.c() as i32 - 1;
    let r: LuaResult<u32> = (|| {
        let newci = match st.pre_call_lua_fast(func, nresults) {
            Some(c) => Some(c),
            None => {
                if st.pre_call_native_fast(func, nresults)? {
                    None
                } else {
                    st.pre_call(func, nresults)?
                }
            }
        };
        Ok(match newci {
            Some(c) => {
                if st.hook_mask() != 0 {
                    c.w().trap = 1;
                }
                H_LUA
            }
            None => H_DONE,
        })
    })();
    finish(&mut st, r)
}

/// BUNDLE-C (`t1ccall`): `OP_CALL` of a light C function (the template checked the tag of the callee slot; hooks, interrupts, `top` and
/// `savedpc` are as for [`jit_call`]). Does what [`jit_call`] does for a native callee (`pre_call_native_fast`, then the general
/// `pre_call` when the stack must grow, no spare `CallInfo` exists, or a hook is set), without the Lua-callee attempt and the
/// closure dispatch, with the result move specialised for the common wanted-result counts. Same return codes as [`jit_call`].
///
/// # Safety
/// As [`jit_call`]; additionally the callee slot holds a light C function.
pub unsafe extern "C" fn jit_ccall(th: *mut Thread, ci: *mut CallInfo, instr: u64, _x: u64) -> u32 {
    // SAFETY: as in `jit_call`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let func = StkId(ci.r().func.0 + 1 + i.a() as usize);
    let nresults = i.c() as i32 - 1;
    let r: LuaResult<u32> = (|| {
        let Some(f) = st.get(func).as_native() else {
            // (not reached: the template guards the tag) the general call
            return Ok(match st.pre_call(func, nresults)? {
                Some(_) => H_LUA,
                None => H_DONE,
            });
        };
        let done = match nresults {
            0 => st.native_fast_with(func, f, 0)?,
            1 => st.native_fast_with(func, f, 1)?,
            2 => st.native_fast_with(func, f, 2)?,
            _ => st.native_fast_with(func, f, nresults)?,
        };
        if !done {
            if let Some(c) = st.pre_call(func, nresults)? {
                if st.hook_mask() != 0 {
                    c.w().trap = 1;
                }
                return Ok(H_LUA);
            }
        }
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// `OP_TAILCALL` (without `top` set: this does it). `instr` is the TAILCALL instruction. The caller has
/// excluded hooks; a `CIST_MMRET` frame is excluded too unless `t1q` bit 5 (BUNDLE-V): then `x` = the `jit_mm_ret` flag word, a refused callee
/// ([`crate::vm_unsafe::jit_mm_tail_ok`]) returns [`H_EXIT`] before anything changed, and a native callee that finished returns [`H_MMPOP`] / [`H_MMFC`].
///
/// # Safety
/// As [`jit_call`].
pub unsafe extern "C" fn jit_tailcall(
    th: *mut Thread,
    ci: *mut CallInfo,
    instr: u64,
    x: u64,
) -> u32 {
    // SAFETY: as in `jit_call`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let ra = StkId(ci.r().func.0 + 1 + i.a() as usize);
    let was_mm = ci.r().callstatus & crate::state::CIST_MMRET != 0;
    if was_mm && !crate::vm_unsafe::jit_mm_tail_ok(&st, ci, st.get(ra)) {
        return H_EXIT; // (nothing has changed: compiled code leaves at the TAILCALL, the interpreter runs it)
    }
    let r: LuaResult<u32> = (|| {
        let mut b = i.b() as usize;
        let nparams1 = i.c() as usize;
        let delta = if nparams1 != 0 {
            ci.r().nextraargs as usize + nparams1
        } else {
            0
        };
        if b != 0 {
            st.set_top(ra + b);
        } else {
            b = st.top() - ra;
        }
        if i.k() != 0 {
            let lvl = StkId(ci.r().func.0 + 1);
            st.close_upvals(lvl);
        }
        crate::jit_hooks::TAIL_FROM_JIT.with(|c| c.set(true));
        let n = st.pre_tail_call(ci, ra, b, delta);
        crate::jit_hooks::TAIL_FROM_JIT.with(|c| c.set(false));
        let n = n?;
        if n < 0 {
            if st.hook_mask() != 0 {
                ci.w().trap = 1;
            }
            return Ok(H_LUA);
        }
        ci.w().func -= delta;
        st.post_call(ci, n as usize)?;
        if was_mm {
            // BUNDLE-V (`t1q` bit 5): the metamethod frame returned the native callee's results: finish the caller's instruction (`x` = the flags of `jit_mm_ret`)
            let v = if n >= 1 && x & 0x400 == 0 { st.get(ci.r().func) } else { Value::NIL }; // (mutant `bv_tail_nil`: x bit 10)
            if crate::vm_unsafe::jit_mm_pop(&mut st, ci, v, x as u32) {
                return Ok(H_MMPOP);
            }
            crate::vm_unsafe::jit_mm_return(&mut st, ci)?;
            return Ok(H_MMFC);
        }
        Ok(H_RETURNED)
    })();
    finish(&mut st, r)
}

// ---- J2a: allocation, table slow paths, concat, closures, pow, len ------------------------------------
//
// Each helper runs the interpreter's code for the part of the instruction that is not an inline fast path
// of the compiled code. Contract (the caller is compiled code, see `templates_alloc.rs`): `ci.savedpc` was
// set exactly like the interpreter's `savepc!()` at that instruction (the address after the instruction and
// its `EXTRAARG`, if it has one); `top` is set here exactly like the interpreter's `Protect` /
// `half_protect` / `check_gc!` do. A helper that returns [`H_EXIT`] has changed nothing but possibly
// `thread.top` and the metamethod-absence flags of metatables (both harmless): compiled code then leaves
// at the instruction and the interpreter redoes it, so a metamethod function (E3), a yield or an error with
// full debug information are the interpreter's. After the helpers that can allocate the stack may have
// moved (GC shrinks it, `check_stack` grows it): compiled code reloads its frame base.

/// `checkGC`: `lim` is the first dead stack index (the caller did `savepc!()`).
fn check_gc(st: &mut State, lim: StkId) {
    if st.g().heap.gc_due() {
        st.set_top(lim);
        st.check_gc();
    }
}

fn reg(st: &State, ci: Ci, n: u32) -> Value {
    st.get(StkId(ci.r().func.0 + 1 + n as usize))
}

/// `K[n]` of the running function.
///
/// # Safety
/// `n` is a valid constant index of the running prototype (compiler guarantee, VM3).
unsafe fn konst(ci: Ci, n: u32) -> Value {
    // SAFETY: caller contract.
    unsafe { Value::load(ci.r().kcache.add(n as usize)) }
}

/// Value of upvalue `n` of the running closure.
///
/// # Safety
/// `n < nupvalues` of the running closure (VM4).
unsafe fn upvalue(st: &State, ci: Ci, n: u32) -> Value {
    // SAFETY: caller contract; `base - 1` holds the running Lua closure (VM2/VM4).
    unsafe {
        let cl = st
            .get(StkId(ci.r().func.0))
            .as_lclosure()
            .unwrap_unchecked();
        Value::load((**cl.upvals_ptr().add(n as usize)).v)
    }
}

/// Test-only mutants of the C2 tier-1 helpers (feature `jit_trace_mut`, set by `rn_jit::tr::mutants::apply_vm_mutants`):
/// bit 0 `c2setupval_barrier_skip`.
#[cfg(feature = "jit_trace_mut")]
static MUT_C2: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Select the C2 helper mutants (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_c2_barrier_skip(on: bool) {
    if on {
        MUT_C2.fetch_or(1, std::sync::atomic::Ordering::Relaxed);
    } else {
        MUT_C2.fetch_and(!1, std::sync::atomic::Ordering::Relaxed);
    }
}


/// Test-only mutants of `jit_mm_pop` (feature `jit_trace_mut`): bit 0 `mmret_nc_skip`, bit 1 `mmret_flag_false`.
#[cfg(feature = "jit_trace_mut")]
static MUT_MM_RET: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Select the `jit_mm_pop` mutant `mmret_nc_skip` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_mm_ret_nc_skip(on: bool) {
    if on {
        MUT_MM_RET.fetch_or(1, std::sync::atomic::Ordering::Relaxed);
    } else {
        MUT_MM_RET.fetch_and(!1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Select the `jit_mm_pop` mutant `mmret_flag_false` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_mm_ret_flag_false(on: bool) {
    if on {
        MUT_MM_RET.fetch_or(2, std::sync::atomic::Ordering::Relaxed);
    } else {
        MUT_MM_RET.fetch_and(!2, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Select the `jit_mm_pop` mutant `armm_dst_off` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_mm_ret_arith_dst(on: bool) {
    if on {
        MUT_MM_RET.fetch_or(4, std::sync::atomic::Ordering::Relaxed);
    } else {
        MUT_MM_RET.fetch_and(!4, std::sync::atomic::Ordering::Relaxed);
    }
}

#[inline(always)]
pub(crate) fn mut_mm_ret(bit: u8) -> bool {
    #[cfg(feature = "jit_trace_mut")]
    return MUT_MM_RET.load(std::sync::atomic::Ordering::Relaxed) & bit != 0;
    #[cfg(not(feature = "jit_trace_mut"))]
    {
        let _ = bit;
        false
    }
}

#[inline(always)]
fn mut_c2(bit: u8) -> bool {
    #[cfg(feature = "jit_trace_mut")]
    return MUT_C2.load(std::sync::atomic::Ordering::Relaxed) & bit != 0;
    #[cfg(not(feature = "jit_trace_mut"))]
    {
        let _ = bit;
        false
    }
}

/// C2: `SETUPVAL` of a collectable value (the interpreter's store plus `luaC_barrier(L, uv, v)`). Never allocates, never fails.
///
/// # Safety
/// Called only from compiled code with the running thread and its current `CallInfo`; `i.b() < nupvalues` (VM4).
pub unsafe extern "C" fn jit_setupval(th: *mut Thread, ci: *mut CallInfo, instr: u64, _x: u64) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`; `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let v = reg(&st, ci, i.a());
    // SAFETY: caller contract; `base - 1` holds the running Lua closure (VM2/VM4).
    unsafe {
        let cl = st.get(StkId(ci.r().func.0)).as_lclosure().unwrap_unchecked();
        let uv = *cl.upvals_ptr().add(i.b() as usize);
        Value::store((*uv).v, v);
        if !mut_c2(1) {
            st.heap().barrier(uv.cast(), &v);
        }
    }
    H_DONE
}

/// C2 / BUNDLE-B: `LT` / `LE` / `EQ` of two registers whose operands the inline number paths of tier 1 did not take (strings,
/// tables, userdata, mixed types ...). [`H_TRUE`] / [`H_FALSE`]: the answer of two strings. [`H_LUA`]: the Lua metamethod
/// (`__lt` / `__le` / `__eq`) was pushed as an in-place frame (`CIST_MMRET`, exactly as the interpreter does; the caller has
/// saved `savedpc` = the instruction after the comparison): compiled code enters it, and its return finishes the comparison
/// ([`jit_mm_ret`]). [`H_EXIT`] otherwise (an error, a C metamethod, a hook, a nearly full C stack: the interpreter takes the
/// instruction); nothing observable has happened then.
///
/// # Safety
/// Called only from compiled code with the running thread and its current `CallInfo` (a Lua frame).
pub unsafe extern "C" fn jit_cmp(th: *mut Thread, ci: *mut CallInfo, instr: u64, x: u64) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`; `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let (a, b) = (reg(&st, ci, i.a()), reg(&st, ci, i.b()));
    let r = |v: bool| if v { H_TRUE } else { H_FALSE };
    match i.opcode() {
        Some(op @ (OpCode::Lt | OpCode::Le)) => match (a.as_string(), b.as_string()) {
            (Some(x), Some(y)) => {
                if op == OpCode::Lt {
                    r(x.as_bytes() < y.as_bytes())
                } else {
                    r(x.as_bytes() <= y.as_bytes())
                }
            }
            _ => {
                if a.is_number() && b.is_number() {
                    return H_EXIT;
                }
                let ev = if op == OpCode::Lt { Tm::Lt } else { Tm::Le };
                match crate::vm_unsafe::jit_mm_order(&mut st, ci, a, b, ev) {
                    Some(_) => H_LUA,
                    None => H_EXIT,
                }
            }
        },
        Some(OpCode::Eq) => {
            if a.is_string() && b.is_string() {
                r(a.raw_equal(&b))
            } else if x & 1 != 0 && a.tag() == b.tag() && !a.is_number() {
                // BUNDLE-Q (`t1q` bit 0; `x` = the CallEnv word, mutants in bits 8..10): everything that needs no metamethod call is answered here.
                if !(a.is_table() || a.is_userdata()) {
                    // functions, threads, light userdata ... (nil / booleans are answered inline): raw equality
                    return r(a.raw_equal(&b));
                }
                if a.raw_equal(&b) {
                    // luaV_equalobj: rawequal objects are equal without any `__eq` lookup
                    return r(x & 0x100 == 0);
                }
                let (m1, _m2) = if let (Some(p), Some(q)) = (a.as_table(), b.as_table()) {
                    (p.metatable(), q.metatable())
                } else if let (Some(p), Some(q)) = (a.as_userdata(), b.as_userdata()) {
                    (p.metatable(), q.metatable())
                } else {
                    return H_EXIT;
                };
                if x & 0x400 != 0 && st.fast_tm(m1, Tm::Eq).is_none() {
                    return H_FALSE; // (mutant `t1q_eq_second_mm_skip`: only the first operand's metatable is asked)
                }
                match crate::vm_unsafe::jit_mm_eq(&mut st, ci, a, b) {
                    Ok(Some(_)) => H_LUA,
                    Ok(None) => r(x & 0x200 != 0),
                    Err(()) => H_EXIT,
                }
            } else if !a.raw_equal(&b) && a.tag() == b.tag() && (a.is_table() || a.is_userdata()) {
                // (raw-equal objects and objects without `__eq` still leave to the interpreter: answering them here changes which loops
                // stay in tier 1 and so the trace set of programs like deltablue, +6 % instructions)
                match crate::vm_unsafe::jit_mm_eq(&mut st, ci, a, b) {
                    Ok(Some(_)) => H_LUA,
                    _ => H_EXIT,
                }
            } else {
                H_EXIT
            }
        }
        _ => H_EXIT,
    }
}

/// BUNDLE-Q (`t1q` bit 1): `MOD` (`R[B] % R[C]`) / `MODK` (`R[B] % K[C]`) of two numbers of which at least one is a float: `luai_nummod` of the two converted
/// to floats (`fmod` plus the sign correction; a zero divisor gives NaN) stored as a float in `R[A]`. [`H_DONE`]; [`H_EXIT`] (nothing changed) for operands that are
/// not numbers or are two integers (the interpreter takes the instruction). `x` = the `CallEnv::t1q` word (mutants in bits 11..13). Never allocates, never fails.
///
/// # Safety
/// Called only from compiled code with the running thread and its current `CallInfo` (a Lua frame).
pub unsafe extern "C" fn jit_fmod(th: *mut Thread, ci: *mut CallInfo, instr: u64, x: u64) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`; `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    // SAFETY: the compiler guarantees the operand indices of a valid instruction (VM3, VM4).
    let (p, q) = unsafe {
        match i.opcode() {
            Some(OpCode::Mod) => (reg(&st, ci, i.b()), reg(&st, ci, i.c())),
            Some(OpCode::Modk) => (reg(&st, ci, i.b()), konst(ci, i.c())),
            _ => return H_EXIT,
        }
    };
    let f = |v: &Value| v.as_float().or_else(|| v.as_int().map(|n| n as f64));
    let (Some(a), Some(b)) = (f(&p), f(&q)) else {
        return H_EXIT;
    };
    if p.is_int() && q.is_int() {
        return H_EXIT;
    }
    let mut m = a % b;
    let fix = if m > 0.0 { b < 0.0 } else { m < 0.0 && b > 0.0 };
    if fix && !(x & 0x800 != 0) && !(x & 0x2000 != 0 && b.is_infinite()) {
        m += b;
    }
    if x & 0x1000 != 0 && m == 0.0 {
        m = 0.0; // (mutant `t1q_mod_negzero`: -0.0 becomes +0.0)
    }
    let base = ci.r().func.0 + 1;
    st.set(StkId(base + i.a() as usize), Value::float(m));
    H_DONE
}

/// BUNDLE-B: `RETURN0` / `RETURN1` (`instr`) of a metamethod frame (`CIST_MMRET`) run in place for a tier-1 comparison: pops the
/// frame ([`crate::vm_unsafe::jit_mm_pop`]) and leaves the result as a boolean at the caller's `top`. [`H_DONE`], or [`H_EXIT`]
/// (nothing changed) when the caller's instruction is not a register comparison. The caller (the template) has excluded hooks.
///
/// # Safety
/// Called only from compiled code with the running thread and its current `CallInfo` (the returning frame).
pub unsafe extern "C" fn jit_mm_ret(th: *mut Thread, ci: *mut CallInfo, instr: u64, x: u64) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`; `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let v = match i.opcode() {
        Some(OpCode::Return1) => reg(&st, ci, i.a()),
        Some(OpCode::Return0) => Value::NIL,
        // BUNDLE-S S1 (`x` bits 1 / 2): the general `RETURN` of a handler that has open upvalues to close (`k`; `return function() ... end` capturing a local): no
        // or one result, not a vararg frame, no to-be-closed variable registered at or above the frame (their `__close` would run Lua code)
        Some(OpCode::Return) if x & 6 != 0 && i.b() <= 2 && i.c() == 0 => {
            let base = ci.r().func.0 + 1;
            if st.jit_tbc_at_or_above(StkId(base)) {
                return H_EXIT;
            }
            if i.b() == 2 { reg(&st, ci, i.a()) } else { Value::NIL }
        }
        // BUNDLE-V (`x` bit 3, `t1q` bit 6): any `RETURN` of a handler frame (several or open results: the first result is the metamethod's value; a vararg frame, e.g. the vararg `__call` constructor of the classic OOP libraries, returns to `func - (nextraargs + C)`)
        Some(OpCode::Return) if x & 8 != 0 => {
            let base = ci.r().func.0 + 1;
            if st.jit_tbc_at_or_above(StkId(base)) {
                return H_EXIT;
            }
            let ra = StkId(base + i.a() as usize);
            let n = if i.b() == 0 { st.top().0.saturating_sub(ra.0) } else { i.b() as usize - 1 };
            if n >= 1 && !(n >= 2 && x & 0x800 != 0) { st.get(ra) } else { Value::NIL } // (mutant `bv_ret_first_nil`: x bit 11)
        }
        _ => return H_EXIT,
    };
    let (func, closes) = (ci.r().func, i.opcode() == Some(OpCode::Return) && i.k() != 0 && x & 0x200 == 0);
    // (a vararg frame: the function slot of the results is `func - (nextraargs + C)`, as in `OP_RETURN`; restored below when the frame is not popped)
    let delta = if i.opcode() == Some(OpCode::Return) && i.c() != 0 { ci.r().nextraargs as usize + i.c() as usize } else { 0 };
    ci.w().func -= delta;

    // (`x` bit 0: the code was lowered with the `armm` limit, arithmetic callers are finished here too; bit 1 / 2: BUNDLE-S, the callers
    // that are table reads / table stores; bits 8..: test-only mutants)
    if crate::vm_unsafe::jit_mm_pop(&mut st, ci, v, x as u32) {
        if closes {
            // (mutant `s1_close_skip`, `x` bit 9: the upvalues of the returned frame stay open)
            st.close_upvals(func + 1);
        }
        H_DONE
    } else {
        ci.w().func += delta;
        H_EXIT
    }
}

/// BUNDLE-D (`armm`): `instr` (the `MMBIN*` after an arithmetic instruction, or a `UNM` / `BNOT`) of the running frame found no number
/// operands in compiled code: push the in-place frame of its Lua metamethod ([`crate::vm_unsafe::jit_mm_arith`]; the caller has saved
/// `savedpc` = the instruction after `instr`). [`H_LUA`]: pushed, compiled code enters it and its return finishes the instruction
/// ([`jit_mm_ret`]); [`H_EXIT`] otherwise, nothing observable has happened (the interpreter runs the arithmetic instruction itself).
///
/// # Safety
/// Called only from compiled code with the running thread and its current `CallInfo` (a Lua frame).
pub unsafe extern "C" fn jit_arith_mm(th: *mut Thread, ci: *mut CallInfo, instr: u64, _x: u64) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`; `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    if crate::vm_unsafe::jit_mm_arith(&mut st, ci, Instruction(instr as u32)) {
        H_LUA
    } else {
        H_EXIT
    }
}

/// BUNDLE-B: the caller of [`jit_mm_ret`] has no compiled continuation: finish its comparison (the `JMP` skip) so that the
/// interpreter can continue it (`FrameChanged`).
///
/// # Safety
/// Called only from compiled code after a successful [`jit_mm_ret`].
pub unsafe extern "C" fn jit_mm_fin(th: *mut Thread, _ci: *mut CallInfo, _instr: u64, _x: u64) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { State::from_th_ptr(th) };
    crate::vm_unsafe::jit_mm_finish(&mut st);
    H_DONE
}

/// `GETTABLE`/`GETI`/`GETFIELD`/`GETTABUP`/`SELF` after the inline probe missed: the raw get plus the
/// `__index` chain (`luaV_finishget`) as long as every `__index` is a table (or a string's metatable);
/// [`H_EXIT`] when a metamethod function, an error or a long chain is involved. Never allocates.
///
/// # Safety
/// Called only from compiled code with the running thread and its current `CallInfo`.
pub unsafe extern "C" fn jit_get(th: *mut Thread, ci: *mut CallInfo, instr: u64, x: u64) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`; `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let a = i.a();
    let Some(op) = i.opcode() else {
        return H_EXIT;
    };
    // SAFETY: the compiler guarantees the operand indices of a valid instruction (VM3, VM4).
    let (t, key) = unsafe {
        match op {
            OpCode::Gettabup => (upvalue(&st, ci, i.b()), konst(ci, i.c())),
            OpCode::Gettable => (reg(&st, ci, i.b()), reg(&st, ci, i.c())),
            OpCode::Geti => (reg(&st, ci, i.b()), Value::int(i64::from(i.c()))),
            OpCode::Getfield | OpCode::SelfOp => (reg(&st, ci, i.b()), konst(ci, i.c())),
            _ => return H_EXIT,
        }
    };
    // P-V: `s:method(...)`: the interpreter's string-metatable fast path (a short-string key)
    if op == OpCode::SelfOp && t.is_string() && key.is_short_string() {
        // SAFETY: the tags were just checked.
        let v = crate::vm_unsafe::index_string_short_str(&st, unsafe { key.strref_unchecked() });
        if v.tag() != crate::value::LUA_VABSTKEY {
            let base = ci.r().func.0 + 1;
            st.set(StkId(base + a as usize + 1), t);
            st.set(StkId(base + a as usize), v);
            return H_DONE;
        }
    }
    // T2: a short-string key of a table (the OOP case: instance miss, then a class chain): the
    // interpreter's `index_chain_short_str` instead of the generic `TableRef::get` walk
    if t.is_table()
        && key.is_short_string()
        && matches!(op, OpCode::Getfield | OpCode::SelfOp | OpCode::Gettabup)
    {
        // SAFETY: the tags were just checked.
        let (h, k) = unsafe { (t.table_unchecked(), key.strref_unchecked()) };
        let mut v = h.get_short_str(k);
        if v.is_nil() {
            v = crate::vm_unsafe::index_chain_short_str(&st, h, k);
        }
        if v.tag() != crate::value::LUA_VABSTKEY {
            let base = ci.r().func.0 + 1;
            if op == OpCode::SelfOp {
                st.set(StkId(base + a as usize + 1), t);
            }
            st.set(StkId(base + a as usize), v);
            return H_DONE;
        }
    }
    let mut obj = t;
    let mut found = None;
    for _ in 0..MAXTAGLOOP {
        let tm;
        if let Some(h) = obj.as_table() {
            let v = h.get(&key);
            if !v.is_nil() {
                found = Some(v);
                break;
            }
            match st.fast_tm(h.metatable(), Tm::Index) {
                None => {
                    found = Some(Value::NIL);
                    break;
                }
                Some(m) => tm = m,
            }
        } else {
            tm = st.get_tm_by_obj(&obj, Tm::Index);
            if tm.is_nil() {
                return H_EXIT; // "attempt to index": the interpreter's message
            }
        }
        if tm.is_function() {
            // BUNDLE-S S1 (`t1q` bit 2; `x` bit 0, mutants in bits 8..): a Lua `__index` function runs as an in-place frame (`CIST_MMRET`, called with the table
            // whose metatable holds it and the key, as the interpreter's `mm_index` does); the caller has saved `savedpc` = the instruction after this one
            if x & 1 != 0 && tm.is_lclosure() {
                if op == OpCode::SelfOp && x & 0x100 == 0 {
                    // (`SELF` stores the receiver before the lookup; mutant `s1_self_skip`: it does not)
                    let base = ci.r().func.0 + 1;
                    st.set(StkId(base + a as usize + 1), t);
                }
                // (mutant `s1_obj_orig`: the first argument is the indexed value, not the level of the chain that holds the function)
                let arg = if x & 0x200 != 0 { t } else { obj };
                return if crate::vm_unsafe::jit_mm_index(&mut st, ci, tm, arg, key) { H_LUA } else { H_EXIT };
            }
            return H_EXIT;
        }
        obj = tm;
    }
    let Some(v) = found else {
        return H_EXIT;
    };
    let base = ci.r().func.0 + 1;
    if op == OpCode::SelfOp {
        st.set(StkId(base + a as usize + 1), t);
    }
    st.set(StkId(base + a as usize), v);
    H_DONE
}

/// `SETTABLE`/`SETI`/`SETFIELD`/`SETTABUP` after the inline probe missed: the interpreter's raw store, then
/// (no `__newindex` in the table) `finish_set_op` = insertion with rehash, `invalidate_tm_cache` and the
/// barrier. [`H_EXIT`] for a non-table target or a `__newindex`. Can allocate (GC, stack moves).
///
/// # Safety
/// As [`jit_get`].
pub unsafe extern "C" fn jit_set(th: *mut Thread, ci: *mut CallInfo, instr: u64, x: u64) -> u32 {
    // SAFETY: as in `jit_get`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let r: LuaResult<u32> = (|| {
        let Some(op) = i.opcode() else {
            return Ok(H_EXIT);
        };
        // SAFETY: operand indices of a valid instruction (VM3, VM4).
        let rc = unsafe {
            if i.k() != 0 {
                konst(ci, i.c())
            } else {
                reg(&st, ci, i.c())
            }
        };
        let (tv, hres): (Value, i32) = match op {
            OpCode::Settabup => {
                // SAFETY: as above.
                let (upv, rb) = unsafe { (upvalue(&st, ci, i.a()), konst(ci, i.b())) };
                let (Some(t), true) = (upv.as_table(), rb.is_string()) else {
                    return Ok(H_EXIT);
                };
                // SAFETY: VM3 — `rb` is a short string.
                let key = unsafe { rb.strref_unchecked() };
                // SAFETY: as above.
                let hres = unsafe { t.psetshortstr(key.as_ptr(), rc) };
                if hres == HOK {
                    st.heap().barrier_back(t.as_ptr().cast(), &rc);
                    return Ok(H_DONE);
                }
                (upv, hres)
            }
            OpCode::Settable => {
                let ra = reg(&st, ci, i.a());
                let rb = reg(&st, ci, i.b());
                let Some(t) = ra.as_table() else {
                    return Ok(H_EXIT);
                };
                if let Some(n) = rb.as_int() {
                    if t.pset_existing_int_arr(n, rc) {
                        st.heap().barrier_back(t.as_ptr().cast(), &rc);
                        return Ok(H_DONE);
                    }
                }
                let hres = match rb.as_int() {
                    Some(n) => t.psetint(n, rc),
                    None => t.pset(rb, rc),
                };
                if hres == HOK {
                    st.heap().barrier_back(t.as_ptr().cast(), &rc);
                    return Ok(H_DONE);
                }
                (ra, hres)
            }
            OpCode::Seti => {
                let ra = reg(&st, ci, i.a());
                let Some(t) = ra.as_table() else {
                    return Ok(H_EXIT);
                };
                let b = i64::from(i.b());
                if t.pset_existing_int_arr(b, rc) {
                    st.heap().barrier_back(t.as_ptr().cast(), &rc);
                    return Ok(H_DONE);
                }
                let hres = t.psetint(b, rc);
                if hres == HOK {
                    st.heap().barrier_back(t.as_ptr().cast(), &rc);
                    return Ok(H_DONE);
                }
                (ra, hres)
            }
            OpCode::Setfield => {
                let ra = reg(&st, ci, i.a());
                // SAFETY: as above.
                let rb = unsafe { konst(ci, i.b()) };
                let (Some(t), true) = (ra.as_table(), rb.is_string()) else {
                    return Ok(H_EXIT);
                };
                // SAFETY: VM3 — `rb` is a short string.
                let key = unsafe { rb.strref_unchecked() };
                // SAFETY: as above.
                let slot = match unsafe { t.pset_existing_shortstr_slot(key.as_ptr(), rc) } {
                    Ok(()) => {
                        st.heap().barrier_back(t.as_ptr().cast(), &rc);
                        return Ok(H_DONE);
                    }
                    Err(slot) => slot,
                };
                // SAFETY: as above; `slot` is that lookup's result.
                let hres = unsafe { t.psetshortstr_after(key.as_ptr(), slot, rc) };
                if hres == HOK {
                    st.heap().barrier_back(t.as_ptr().cast(), &rc);
                    return Ok(H_DONE);
                }
                (ra, hres)
            }
            _ => return Ok(H_EXIT),
        };
        let h = tv.as_table().expect("table checked above");
        if let Some(tm) = st.fast_tm(h.metatable(), Tm::NewIndex) {
            // BUNDLE-S S1 (`t1q` bit 3; `x` bit 1): a Lua `__newindex` function runs as an in-place frame (`savedpc` = the instruction after this one, saved by the
            // caller); anything else (a table, a C function, a hook, the C-stack limit) is the interpreter's
            if x & 2 != 0 && tm.is_lclosure() && crate::vm_unsafe::jit_mm_newindex(&mut st, ci, i, tv) {
                return Ok(H_LUA);
            }
            return Ok(H_EXIT); // `__newindex`: the interpreter
        }
        // `Protect`: top = ci.top, then the general store (insertion, rehash, barrier)
        let t = ci.r().top;
        st.set_top(t);
        finish_set_op(&mut st, ci, i, hres)?;
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// `NEWTABLE` (`x` = the `EXTRAARG` word's `ax` when `k` is set). `savedpc` = after the `EXTRAARG`.
///
/// # Safety
/// As [`jit_get`].
pub unsafe extern "C" fn jit_newtable(
    th: *mut Thread,
    ci: *mut CallInfo,
    instr: u64,
    x: u64,
) -> u32 {
    // SAFETY: as in `jit_get`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let r: LuaResult<u32> = (|| {
        #[cfg(not(rustynail_lua54))]
        let (mut b, mut c, cmul) = (i.vb(), i.vc(), crate::instruction::MAXARG_VC + 1);
        #[cfg(rustynail_lua54)]
        let (mut b, mut c, cmul) = (i.b(), i.c(), crate::instruction::MAXARG_C + 1);
        if b > 0 {
            b = 1u32 << (b - 1); // hash size is 2^(b - 1)
        }
        if i.k() != 0 {
            c = c.wrapping_add((x as u32).wrapping_mul(cmul));
        }
        let ra = StkId(ci.r().func.0 + 1 + i.a() as usize);
        st.set_top(ra + 1); // correct top in case of emergency GC
        let t = st.new_table()?;
        st.set(ra, Value::table(t));
        if b != 0 || c != 0 {
            if let Err(e) = t.resize(st.heap(), c, b) {
                return Err(st.table_error(e));
            }
        }
        check_gc(&mut st, ra + 1);
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// `SETLIST` (`x` = `ax` of the `EXTRAARG` when `k` is set). `savedpc` = after the `EXTRAARG`.
///
/// # Safety
/// As [`jit_get`].
pub unsafe extern "C" fn jit_setlist(
    th: *mut Thread,
    ci: *mut CallInfo,
    instr: u64,
    x: u64,
) -> u32 {
    // SAFETY: as in `jit_get`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let r: LuaResult<u32> = (|| {
        #[cfg(not(rustynail_lua54))]
        let (mut n, mut last, cmul) = (
            i.vb() as usize,
            i.vc() as usize,
            crate::instruction::MAXARG_VC as usize + 1,
        );
        #[cfg(rustynail_lua54)]
        let (mut n, mut last, cmul) = (
            i.b() as usize,
            i.c() as usize,
            crate::instruction::MAXARG_C as usize + 1,
        );
        let ra = StkId(ci.r().func.0 + 1 + i.a() as usize);
        let Some(h) = st.get(ra).as_table() else {
            return Err(st.run_error(b"internal error: SETLIST on a non-table"));
        };
        if n == 0 {
            n = (st.top() - ra) - 1; // get up to the top
        } else {
            let t = ci.r().top;
            st.set_top(t); // correct top in case of emergency GC
        }
        last += n;
        if i.k() != 0 {
            last += x as u32 as usize * cmul;
        }
        // when 'n' is known, table should have proper size
        if last > h.array_size() as usize {
            if let Err(e) = h.resize_array(st.heap(), last as u32) {
                return Err(st.table_error(e));
            }
        }
        while n > 0 {
            let val = st.get(ra + n);
            if let Err(e) = h.set_int(st.heap(), last as i64, &val) {
                return Err(st.table_error(e));
            }
            last -= 1;
            st.heap().barrier_back(h.as_ptr().cast(), &val);
            n -= 1;
        }
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// `LEN`: tables without `__len` and strings; [`H_EXIT`] otherwise. Does not allocate.
///
/// # Safety
/// As [`jit_get`].
pub unsafe extern "C" fn jit_len(th: *mut Thread, ci: *mut CallInfo, instr: u64, _x: u64) -> u32 {
    // SAFETY: as in `jit_get`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let rb = reg(&st, ci, i.b());
    let n = if let Some(t) = rb.as_table() {
        if !TableRef::no_tm(t.metatable(), Tm::Len) {
            return H_EXIT;
        }
        t.len(st.heap()) as i64
    } else if let Some(s) = rb.as_string() {
        s.len() as i64
    } else {
        return H_EXIT;
    };
    st.set(StkId(ci.r().func.0 + 1 + i.a() as usize), Value::int(n));
    H_DONE
}

/// `CONCAT A B` when every operand is a string or a number (no metamethod can run); otherwise [`H_EXIT`].
/// `savedpc` = after the instruction.
///
/// # Safety
/// As [`jit_get`].
pub unsafe extern "C" fn jit_concat(
    th: *mut Thread,
    ci: *mut CallInfo,
    instr: u64,
    _x: u64,
) -> u32 {
    // SAFETY: as in `jit_get`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let n = i.b() as usize; // number of elements to concatenate
    let ra = StkId(ci.r().func.0 + 1 + i.a() as usize);
    for j in 0..n {
        let v = st.get(ra + j);
        if !(v.is_string() || v.is_number()) {
            return H_EXIT;
        }
    }
    let r: LuaResult<u32> = (|| {
        st.set_top(ra + n); // mark the end of concat operands
        st.concat(n)?;
        let t = st.top();
        check_gc(&mut st, t); // 'concat' ensures correct top
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// `CLOSURE A Bx`. `savedpc` = after the instruction.
///
/// # Safety
/// As [`jit_get`].
pub unsafe extern "C" fn jit_closure(
    th: *mut Thread,
    ci: *mut CallInfo,
    instr: u64,
    _x: u64,
) -> u32 {
    // SAFETY: as in `jit_get`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let r: LuaResult<u32> = (|| {
        let base = ci.r().func.0 + 1;
        // SAFETY: VM2/VM4 — `base - 1` holds the running Lua closure.
        let cl = unsafe { st.get(StkId(base - 1)).as_lclosure().unwrap_unchecked() };
        let sub = cl.proto().protos()[i.bx() as usize];
        let t = ci.r().top;
        st.set_top(t); // `half_protect`
        let ra = StkId(base + i.a() as usize);
        push_closure(&mut st, cl, sub, base, ra)?;
        check_gc(&mut st, ra + 1);
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// `POW` / `POWK`: both operands numbers, else [`H_EXIT`]. Does not allocate.
///
/// # Safety
/// As [`jit_get`].
pub unsafe extern "C" fn jit_pow(th: *mut Thread, ci: *mut CallInfo, instr: u64, _x: u64) -> u32 {
    // SAFETY: as in `jit_get`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let p1 = reg(&st, ci, i.b());
    // SAFETY: VM3 — `c` is a constant index for POWK.
    let p2 = unsafe {
        if i.opcode() == Some(OpCode::Powk) {
            konst(ci, i.c())
        } else {
            reg(&st, ci, i.c())
        }
    };
    if let (Some(x), Some(y)) = (tonum(&p1), tonum(&p2)) {
        st.set(
            StkId(ci.r().func.0 + 1 + i.a() as usize),
            Value::float(crate::number::pow_lua(x, y)),
        );
        return H_DONE;
    }
    H_EXIT
}

/// `OP_VARARGPREP` (`luaT_adjustvarargs`; compiled code has excluded active hooks and set `ci.savedpc` to the
/// instruction after it). Moves the function and the fixed parameters above the extra arguments (and builds
/// the vararg table of a named-vararg function), so `ci.func`/`ci.top` change: compiled code reloads `rbx`.
///
/// # Safety
/// As [`jit_call`]; `ci` is a Lua frame at its `VARARGPREP` with `top` = end of the arguments.
pub unsafe extern "C" fn jit_varargprep(
    th: *mut Thread,
    ci: *mut CallInfo,
    _instr: u64,
    _x: u64,
) -> u32 {
    // SAFETY: as in `jit_call`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let r: LuaResult<u32> = (|| {
        let f = st.get(ci.r().func);
        let Some(cl) = f.as_lclosure() else {
            return Err(st.run_error(b"internal error: not a Lua frame"));
        };
        st.adjust_varargs(ci, cl.proto())?;
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// `OP_VARARG` (`protect!`: `top = ci.top`, then `get_varargs`).
///
/// # Safety
/// As [`jit_call`].
pub unsafe extern "C" fn jit_vararg(
    th: *mut Thread,
    ci: *mut CallInfo,
    instr: u64,
    _x: u64,
) -> u32 {
    // SAFETY: as in `jit_call`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let r: LuaResult<u32> = (|| {
        let t = ci.r().top;
        st.set_top(t);
        let n = i.c() as i32 - 1; // required results (-1 means all)
        #[cfg(not(rustynail_lua54))]
        let vatab = if i.k() != 0 { i.b() as i32 } else { -1 };
        #[cfg(rustynail_lua54)]
        let vatab = -1;
        let ra = StkId(ci.r().func.0 + 1 + i.a() as usize);
        st.get_varargs(ci, ra, n, vatab)?;
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// `OP_CLOSE` (`protect!`): closes upvalues and to-be-closed variables from register `A`; a `__close`
/// metamethod runs as a nested call (it may yield: the status comes back as an error exit).
///
/// # Safety
/// As [`jit_call`].
pub unsafe extern "C" fn jit_close(th: *mut Thread, ci: *mut CallInfo, instr: u64, _x: u64) -> u32 {
    // SAFETY: as in `jit_call`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let r: LuaResult<u32> = (|| {
        let t = ci.r().top;
        st.set_top(t);
        let level = StkId(ci.r().func.0 + 1 + i.a() as usize);
        st.close(level, crate::status::Status::Ok as u8, true)?;
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// `OP_TBC` (`halfProtect`: `top = ci.top`, then `new_tbc_upval`; raises "got a non-closable value").
///
/// # Safety
/// As [`jit_call`].
pub unsafe extern "C" fn jit_tbc(th: *mut Thread, ci: *mut CallInfo, instr: u64, _x: u64) -> u32 {
    // SAFETY: as in `jit_call`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let r: LuaResult<u32> = (|| {
        let t = ci.r().top;
        st.set_top(t);
        st.new_tbc_upval(StkId(ci.r().func.0 + 1 + i.a() as usize))?;
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// The closing part of `OP_RETURN` with the `k` bit: records the result count in `ci.u2`, raises `top` to
/// `ci.top` if needed, closes everything from register 0, and restores `top = ra + n`. The template then
/// performs the return itself (so `top` and the results are exactly as the interpreter leaves them).
///
/// # Safety
/// As [`jit_call`]; `instr` is the `RETURN` instruction.
pub unsafe extern "C" fn jit_return_close(
    th: *mut Thread,
    ci: *mut CallInfo,
    instr: u64,
    _x: u64,
) -> u32 {
    // SAFETY: as in `jit_call`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let r: LuaResult<u32> = (|| {
        let ra = StkId(ci.r().func.0 + 1 + i.a() as usize);
        let mut n = i.b() as i32 - 1;
        if n < 0 {
            n = (st.top() - ra) as i32;
        }
        ci.w().u2 = n;
        let t = ci.r().top;
        if st.top() < t {
            st.set_top(t);
        }
        let level = StkId(ci.r().func.0 + 1);
        st.close(level, crate::func::CLOSEKTOP, true)?;
        // (the stack may have moved, offsets did not)
        st.set_top(ra + n as usize);
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}

/// Writes `nres` results (`k`, `v`, then nil padding) at `res`.
#[inline(always)]
fn tfor_results2(st: &mut State, res: StkId, nres: usize, k: Value, v: Value) {
    if nres >= 1 {
        st.set(res, k);
    }
    if nres >= 2 {
        st.set(res + 1, v);
        for j in 2..nres {
            st.set(res + j, Value::NIL);
        }
    }
}

/// Writes `nres` results (a single nil, padded with nil) at `res`.
#[inline(always)]
fn tfor_nil(st: &mut State, res: StkId, nres: usize) {
    for j in 0..nres {
        st.set(res + j, Value::NIL);
    }
}

/// Runs `next` / the `ipairs` iterator for `OP_TFORCALL` without a call frame. `builtins` = the addresses of
/// `next` and of the `ipairs` iterator (0 = none). `false` = not applicable (another function, other
/// state/control types, a metamethod could run, or an invalid key: the generic call then produces the exact
/// behaviour and error).
#[inline(always)]
pub(crate) fn tfor_fast(st: &mut State, ra: StkId, nres: usize, builtins: (usize, usize)) -> bool {
    // The interpreter's `OP_TFORCALL` goes through `ccall` (`nCcalls + 1`, "C stack overflow" at the limit):
    // near the limit the generic call must run so that the error is raised at the same depth.
    if crate::state::get_ccalls(st.ncalls_raw()) + 1 >= crate::state::LUAI_MAXCCALLS {
        return false;
    }
    let f = st.get(ra);
    let Some(nf) = f.as_native() else {
        return false;
    };
    let nf = nf as usize;
    let Some(t) = st.get(ra + 1).as_table() else {
        return false;
    };
    // 5.5: control variable and results at `ra + 3`; 5.4: control at `ra + 2`, results from `ra + 4`
    #[cfg(not(rustynail_lua54))]
    let (ctl_at, res) = (ra + 3, ra + 3);
    #[cfg(rustynail_lua54)]
    let (ctl_at, res) = (ra + 2, ra + 4);
    let ctl = st.get(ctl_at);
    if nf != 0 && nf == builtins.0 {
        let mut hint = st.heap().next_hint;
        let r = t.next_hinted(&ctl, &mut hint);
        st.heap().next_hint = hint;
        match r {
            Ok(Some((k, v))) => tfor_results2(st, res, nres, k, v),
            Ok(None) => tfor_nil(st, res, nres),
            Err(_) => return false,
        }
        true
    } else if nf != 0 && nf == builtins.1 {
        let Some(c) = ctl.as_int() else {
            return false;
        };
        let i = c.wrapping_add(1);
        let v = match t.get_int_arr(i) {
            Some(v) => v,
            None => t.get_int(i),
        };
        if v.is_nil() {
            if t.metatable().is_some() {
                return false; // `__index` may run
            }
            tfor_nil(st, res, nres);
        } else {
            tfor_results2(st, res, nres, Value::int(i), v);
        }
        true
    } else {
        false
    }
}

/// `OP_TFORCALL` (`protect_nt!`: only `savedpc` was stored). `next` and the `ipairs` iterator over a table run
/// without a frame (same results as the builtins); everything else is the interpreter's nested `call`.
///
/// # Safety
/// As [`jit_call`].
pub unsafe extern "C" fn jit_tforcall(
    th: *mut Thread,
    ci: *mut CallInfo,
    instr: u64,
    _x: u64,
) -> u32 {
    // SAFETY: as in `jit_call`.
    let (mut st, ci) = unsafe { (State::from_th_ptr(th), Ci::from_raw(ci)) };
    let i = Instruction(instr as u32);
    let r: LuaResult<u32> = (|| {
        let ra = StkId(ci.r().func.0 + 1 + i.a() as usize);
        let nres = i.c() as usize;
        if st.hook_mask() == 0
            && tfor_fast(
                &mut st,
                ra,
                nres,
                (
                    crate::jit_hooks::builtin_next(),
                    crate::jit_hooks::builtin_ipairs_aux(),
                ),
            )
        {
            return Ok(H_DONE);
        }
        #[cfg(not(rustynail_lua54))]
        {
            let v = st.get(ra + 3);
            st.set(ra + 5, v); // copy the control variable
            let v = st.get(ra + 1);
            st.set(ra + 4, v); // copy state
            let v = st.get(ra);
            st.set(ra + 3, v); // copy function
            st.set_top(ra + 6);
            st.call(ra + 3, nres as i32)?;
        }
        #[cfg(rustynail_lua54)]
        {
            let v = st.get(ra);
            st.set(ra + 4, v);
            let v = st.get(ra + 1);
            st.set(ra + 5, v);
            let v = st.get(ra + 2);
            st.set(ra + 6, v);
            st.set_top(ra + 7);
            st.call(ra + 4, nres as i32)?;
        }
        Ok(H_DONE)
    })();
    finish(&mut st, r)
}
