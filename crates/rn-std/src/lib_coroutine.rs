//! Library `coroutine` (port of `lcorolib.c`) over `State::resume` / `yield_k`.
#![forbid(unsafe_code)]

use rn_core::coro::ResumeOut;
use rn_core::state::{NativeFn, State};
use rn_core::status::{LuaResult, Status};
use rn_core::value::{Gc, LuaType, Value};

fn getco(st: &mut State) -> LuaResult<State> {
    match st.arg(0).as_thread() {
        Some(t) => Ok(State::from_thread(t)),
        None => Err(st.type_error_arg(1, b"thread")),
    }
}

/// Resumes a coroutine. `Ok(Some(n))`: `n` results are on top of `st`; `Ok(None)`: an error
/// message was pushed on `st`.
#[inline(always)]
fn auxresume(st: &mut State, co: &mut State, narg: usize) -> LuaResult<Option<usize>> {
    match co.resume_fused(st, narg) {
        ResumeOut::Ok(n) => Ok(Some(n)),
        ResumeOut::Err => Ok(None),
        ResumeOut::TooMany => {
            st.push_bytes(b"too many results to resume")?;
            Ok(None)
        }
        ResumeOut::Slow => auxresume_slow(st, co, narg),
    }
}

#[inline(never)]
fn auxresume_slow(st: &mut State, co: &mut State, narg: usize) -> LuaResult<Option<usize>> {
    if !co.check_stack_api(narg) {
        st.push_bytes(b"too many arguments to resume")?;
        return Ok(None);
    }
    st.xmove(co, narg);
    let (status, nres) = co.resume(Some(&*st), narg);
    if status == Status::Ok || status == Status::Yield {
        if !st.check_stack_api(nres + 1) {
            let t = co.top() - nres;
            co.set_top(t); // remove results anyway
            st.push_bytes(b"too many results to resume")?;
            return Ok(None);
        }
        co.xmove(st, nres); // move yielded values
        Ok(Some(nres))
    } else {
        co.xmove(st, 1); // move error message
        Ok(None)
    }
}

fn co_resume(st: &mut State) -> LuaResult<usize> {
    let mut co = getco(st)?;
    let narg = st.nargs() - 1;
    match auxresume(st, &mut co, narg)? {
        None => {
            let msg = st.pop();
            st.push(Value::FALSE);
            st.push(msg);
            Ok(2) // false + error message
        }
        Some(r) => {
            // true + the values returned by `resume`: shift the `r` values up by one slot
            // (auxresume checked that there is room for `r + 1`), no temporary allocation
            if r == 0 {
                st.push(Value::TRUE);
            } else {
                let first = st.top() - r;
                let last = st.get(st.top() - 1);
                st.push(last);
                for i in (0..r - 1).rev() {
                    let v = st.get(first + i);
                    st.set(first + i + 1, v);
                }
                st.set(first, Value::TRUE);
            }
            Ok(r + 1)
        }
    }
}

fn auxwrap(st: &mut State) -> LuaResult<usize> {
    let func = st.ci().r().func;
    let th = st
        .get(func)
        .as_cclosure()
        .and_then(|cl| cl.upvalue(0).as_thread());
    let Some(th) = th else {
        return Err(st.l_error(b"invalid coroutine function"));
    };
    let mut co = State::from_thread(th);
    let narg = st.nargs();
    match auxresume(st, &mut co, narg)? {
        Some(r) => Ok(r),
        None => {
            let mut stat = co.status();
            if stat != Status::Ok && stat != Status::Yield {
                // error in the coroutine: close its tbc variables
                stat = co.close_thread(Some(&*st));
                co.xmove(st, 1); // move error message to the caller
            }
            if stat != Status::ErrMem {
                if let Some(s) = st.get(st.top() - 1).as_string() {
                    // error object is a string: add extra info, if available
                    let mut full = st.where_(1);
                    full.extend_from_slice(s.as_bytes());
                    st.pop();
                    st.push_bytes(&full)?;
                }
            }
            Err(st.error_msg()) // propagate error (the object is on top)
        }
    }
}

fn co_create_aux(st: &mut State) -> LuaResult<Gc<rn_core::state::Thread>> {
    st.check_type(1, LuaType::Function)?;
    let nl = st.new_thread()?;
    let f = st.arg(0);
    st.push(f); // move function to top
    let mut nls = State::from_thread(nl);
    st.xmove(&mut nls, 1); // move function from L to NL
    Ok(nl)
}

fn co_create(st: &mut State) -> LuaResult<usize> {
    co_create_aux(st)?;
    Ok(1)
}

fn co_wrap(st: &mut State) -> LuaResult<usize> {
    let th = co_create_aux(st)?;
    let cl = st.try_alloc(|h| h.new_cclosure(auxwrap, 1))?;
    cl.set_upvalue(st.heap(), 0, Value::thread(th));
    st.push(Value::cclosure(cl));
    st.check_gc();
    Ok(1)
}

fn co_yield(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    st.yield_k(n, 0, None)
}

const COS_RUN: usize = 0;
const COS_DEAD: usize = 1;
const COS_YIELD: usize = 2;
const COS_NORM: usize = 3;
const STATNAME: [&str; 4] = ["running", "dead", "suspended", "normal"];

fn auxstatus(st: &State, co: &State) -> usize {
    if st.same_thread(co) {
        return COS_RUN;
    }
    match co.status() {
        Status::Yield => COS_YIELD,
        Status::Ok => {
            if co.get_stack(0).is_some() {
                COS_NORM // it has frames: it is running (it resumed another one)
            } else if co.frame_top() == 0 {
                COS_DEAD
            } else {
                COS_YIELD // initial state
            }
        }
        _ => COS_DEAD, // some error occurred
    }
}

fn co_status(st: &mut State) -> LuaResult<usize> {
    let co = getco(st)?;
    let s = auxstatus(st, &co);
    st.push_str(STATNAME[s])?;
    Ok(1)
}

fn getoptco(st: &mut State) -> LuaResult<State> {
    if st.nargs() == 0 {
        Ok(State::from_thread(st.as_gc()))
    } else {
        getco(st)
    }
}

fn co_yieldable(st: &mut State) -> LuaResult<usize> {
    let co = getoptco(st)?;
    st.push(Value::boolean(co.yieldable()));
    Ok(1)
}

fn co_running(st: &mut State) -> LuaResult<usize> {
    let th = st.as_gc();
    st.push(Value::thread(th));
    let ismain = st.is_main_thread();
    st.push(Value::boolean(ismain));
    Ok(2)
}

fn co_close(st: &mut State) -> LuaResult<usize> {
    #[cfg(not(rustynail_lua54))]
    let mut co = getoptco(st)?;
    #[cfg(rustynail_lua54)]
    let mut co = getco(st)?;
    let status = auxstatus(st, &co);
    match status {
        COS_DEAD | COS_YIELD => {
            let s = co.close_thread(Some(&*st));
            if s == Status::Ok {
                st.push(Value::TRUE);
                Ok(1)
            } else {
                st.push(Value::FALSE);
                co.xmove(st, 1); // move error message
                Ok(2)
            }
        }
        COS_NORM => Err(st.l_error(b"cannot close a normal coroutine")),
        #[cfg(rustynail_lua54)]
        _ => Err(st.l_error(b"cannot close a running coroutine")),
        #[cfg(not(rustynail_lua54))]
        _ => {
            // COS_RUN
            if co.is_main_thread() {
                return Err(st.l_error(b"cannot close main thread"));
            }
            let s = co.close_thread(None); // close itself
                                           // `lua_closethread` throws `s` to the base level when closing itself (`Ok` is a
                                           // "successful" throw that the nearest protected call treats as done)
            Err(st.throw_base_level(s))
        }
    }
}

pub fn open(st: &mut State) -> LuaResult<usize> {
    let lib = st.new_table_sized(0, 8)?;
    st.push(Value::table(lib));
    let funcs: &[(&str, Option<NativeFn>)] = &[
        ("create", Some(co_create)),
        ("resume", Some(co_resume)),
        ("running", Some(co_running)),
        ("status", Some(co_status)),
        ("wrap", Some(co_wrap)),
        ("yield", Some(co_yield)),
        ("isyieldable", Some(co_yieldable)),
        ("close", Some(co_close)),
    ];
    st.set_funcs(lib, funcs)?;
    Ok(1)
}
