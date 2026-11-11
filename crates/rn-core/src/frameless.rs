//! T2: the interpreter's generic `for` runs `next` / the `ipairs` iterator without a `CallInfo`.
//!
//! The addresses are registered by the base library ([`set_interp_builtins`]). They are used only while no
//! compiler registered (`jit_hooks::set_builtins` calls [`disable`]): with a compiler the trace recorder models
//! the stack image of the real call and a trace entry needs the spare `CallInfo` node that call leaves behind
//! (TR6b), and the tier-1 helper has its own fast path.

use crate::state::{State, StkId};
use crate::value::Value;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static INTERP_NEXT: AtomicUsize = AtomicUsize::new(0);
static INTERP_IPAIRS_AUX: AtomicUsize = AtomicUsize::new(0);
static DISABLED: AtomicBool = AtomicBool::new(false);

/// Registers the addresses of `next` and of the `ipairs` iterator.
pub fn set_interp_builtins(next: usize, ipairs_aux: usize) {
    INTERP_NEXT.store(next, Ordering::Relaxed);
    INTERP_IPAIRS_AUX.store(ipairs_aux, Ordering::Relaxed);
}

/// A compiler registered: the interpreter calls the iterators again.
#[allow(dead_code)]
pub(crate) fn disable() {
    DISABLED.store(true, Ordering::Relaxed);
}

#[inline(always)]
fn interp_builtins() -> (usize, usize) {
    if DISABLED.load(Ordering::Relaxed) {
        return (0, 0);
    }
    (
        INTERP_NEXT.load(Ordering::Relaxed),
        INTERP_IPAIRS_AUX.load(Ordering::Relaxed),
    )
}

/// Writes `nres` results (`k`, `v`, then nil padding) at `res`.
#[inline(always)]
fn results2(st: &mut State, res: StkId, nres: usize, k: Value, v: Value) {
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

#[inline(always)]
fn nils(st: &mut State, res: StkId, nres: usize) {
    for j in 0..nres {
        st.set(res + j, Value::NIL);
    }
}

/// T2: `call(func, nres)` of the generic `for`: `func` holds `next` or the `ipairs` iterator, `func + 1` the
/// state (a table) and `func + 2` the control variable (both 5.5 and 5.4 lay the call out this way). The
/// results are what the builtin returns, adjusted to `nres` and placed at `func` with `top = func + nres`,
/// exactly like `post_call`; no `CallInfo`, no hooks (a call hook would see the call). `false` = not
/// applicable: nothing was changed and the real call must run (it raises the errors, runs `__index`).
#[inline]
pub(crate) fn call_frameless_iter(st: &mut State, func: StkId, nres: usize) -> bool {
    let (b_next, b_ipairs) = interp_builtins();
    if b_next == 0 {
        return false; // the common case of every other call: one load and compare
    }
    let Some(nf) = st.get(func).as_native() else {
        return false;
    };
    let nf = nf as usize;
    if (nf != b_next && nf != b_ipairs)
        || st.hook_mask() != 0
        || crate::state::get_ccalls(st.ncalls_raw()) + 1 >= crate::state::LUAI_MAXCCALLS
    {
        return false;
    }
    let Some(t) = st.get(func + 1).as_table() else {
        return false;
    };
    let ctl = st.get(func + 2);
    let kv = if nf == b_next {
        let mut hint = st.heap().next_hint;
        let r = t.next_hinted(&ctl, &mut hint);
        st.heap().next_hint = hint;
        match r {
            Ok(r) => r,
            Err(_) => return false,
        }
    } else {
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
            None
        } else {
            Some((Value::int(i), v))
        }
    };
    match kv {
        Some((k, v)) => results2(st, func, nres, k, v),
        None => nils(st, func, nres),
    }
    st.set_top(func + nres);
    true
}
