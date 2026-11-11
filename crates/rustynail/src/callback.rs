//! Rust functions and userdata inside Lua: the call trampoline, the finalizer that drops Rust
//! values, the instruction-limit hook and the message handler.
//!
//! A Rust closure becomes a native closure (`CClosure`) whose function is [`call_rust`] and whose
//! upvalues are: (1) the integer id of the boxed closure in the per-state table
//! ([`crate::shared`]), (2) an *owner* userdata with the same id as user value 1 and a metatable
//! whose `__gc` is [`drop_object`], then the closure's own Lua upvalues. A typed userdata is a
//! zero-length userdata with the id as user value 1 and the metatable of its Rust type (again with
//! `__gc` = [`drop_object`]). No Rust data is ever stored in Lua memory.
use crate::error::{Error, ErrorKind, Result};
use crate::shared;
use crate::stack::{catch, Handle, Stack};
use crate::ThreadStatus;
use rn_core::state::State;
use rn_core::status::{LuaResult, Status};
use rn_core::table::TableRef;
use rn_core::value::Value;
use std::any::{Any, TypeId};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

/// The message of the instruction-limit error.
pub(crate) const LIMIT_MSG: &str = "instruction limit exceeded";

type BoxedFn = Box<dyn Fn(&mut dyn Stack) -> Result<usize>>;

/// A Rust function owned by Lua.
pub(crate) struct Callback(pub BoxedFn);

/// Index of the first user upvalue of a Rust closure.
const FIRST_UPVALUE: usize = 2;

fn mem_error() -> Error {
    Error::new(ErrorKind::Memory, "not enough memory")
}

fn panic_message(p: &(dyn Any + Send)) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "(non-string panic payload)".to_string()
    }
}

/// The id stored as user value 1 of a facade userdata (closure owner or typed userdata).
fn udata_id(v: &Value) -> Option<i64> {
    v.as_userdata()?.user_value(0)?.as_int()
}

/// The Rust object of a typed userdata (cloned `Rc`).
pub(crate) fn userdata_object(st: &State, v: &Value) -> Option<Rc<dyn Any>> {
    let id = udata_id(v)?;
    shared::get(st, id)
}

/// Was `v` created by [`crate::Stack::push_userdata`] (or is it a closure owner)?
pub(crate) fn is_rust_userdata(st: &State, v: &Value) -> bool {
    udata_id(v).is_some_and(|id| shared::get(st, id).is_some())
}

/// `__gc` of every facade userdata: drop the Rust object (outside the table borrow, catching a
/// panic of its `Drop`).
fn drop_object(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    if let Some(id) = udata_id(&v) {
        if let Some(obj) = shared::remove(st, id) {
            let _ = catch_unwind(AssertUnwindSafe(move || drop(obj)));
        }
    }
    Ok(0)
}

/// A metatable with `__gc` = [`drop_object`] (and the given extra string fields), stored in the
/// registry; returns its reference.
fn make_metatable(st: &mut State, name: Option<&str>, protect_mt: bool) -> Result<i32> {
    let old_top = st.top();
    let mut r = 0;
    let status = st.pcall_with(
        |st| {
            st.check_stack(2)?;
            let t = st.new_table()?;
            st.push(Value::table(t));
            st.raw_set_str(t, b"__gc", Value::native(drop_object))?;
            if let Some(n) = name {
                let s = st.new_str_value(n.as_bytes())?;
                st.raw_set_str(t, b"__name", s)?;
            }
            if protect_mt {
                st.raw_set_str(t, b"__metatable", Value::FALSE)?;
            }
            let reg = st.registry();
            r = st.ref_(reg, Value::table(t))?;
            st.set_top(old_top);
            Ok(())
        },
        old_top,
        0,
    );
    if status == Status::Ok {
        Ok(r)
    } else {
        Err(catch(st, status, false))
    }
}

fn registry_table(st: &State, r: i32) -> Option<TableRef> {
    st.registry().get_int(i64::from(r)).as_table()
}

/// The metatable of the closure owners (created on first use).
fn callback_metatable(st: &mut State) -> Result<TableRef> {
    let r = shared::with(st, |sh| sh.callback_mt).unwrap_or(0);
    if r > 0 {
        if let Some(t) = registry_table(st, r) {
            return Ok(t);
        }
    }
    let r = make_metatable(st, Some("rustynail.function"), true)?;
    shared::with(st, |sh| sh.callback_mt = r);
    registry_table(st, r).ok_or_else(|| Error::api("lost the closure metatable"))
}

/// The metatable of the userdata of type `T` (created on first use).
pub(crate) fn type_metatable<T: 'static>(st: &mut State) -> Result<TableRef> {
    let tid = TypeId::of::<T>();
    let r = shared::with(st, |sh| sh.type_mts.get(&tid).copied())
        .flatten()
        .unwrap_or(0);
    if r > 0 {
        if let Some(t) = registry_table(st, r) {
            return Ok(t);
        }
    }
    let r = make_metatable(st, Some(std::any::type_name::<T>()), false)?;
    shared::with(st, |sh| sh.type_mts.insert(tid, r));
    registry_table(st, r).ok_or_else(|| Error::api("lost the userdata metatable"))
}

/// Push a zero-length userdata owning object `id` with metatable `mt`. On failure the object is
/// removed again.
fn push_owner(st: &mut State, id: i64, mt: TableRef) -> Result<Value> {
    let u = match st.new_udata(0, 1) {
        Ok(u) => u,
        Err(_) => {
            drop(shared::remove(st, id));
            return Err(mem_error());
        }
    };
    let uv = Value::userdata(u);
    st.push(uv);
    u.set_user_value(st.heap(), 0, Value::int(id));
    st.set_metatable(&uv, Some(mt));
    Ok(uv)
}

/// Push a Rust closure (`obj` is the boxed [`Callback`]) with `nup` upvalues taken from the
/// stack. The caller reserved 2 slots.
pub(crate) fn new_closure(st: &mut State, obj: Rc<dyn Any>, nup: usize) -> Result<()> {
    let mt = callback_metatable(st)?;
    let id = shared::insert(st, obj).ok_or_else(|| Error::api("not a rustynail state"))?;
    let owner = push_owner(st, id, mt)?;
    // from here on the owner's finalizer releases the object, whatever happens
    let cl = match st.try_alloc(|h| h.new_cclosure(call_rust, FIRST_UPVALUE + nup)) {
        Ok(c) => c,
        Err(_) => {
            let t = st.top();
            st.set_top(t - 1);
            return Err(mem_error());
        }
    };
    cl.set_upvalue(st.heap(), 0, Value::int(id));
    cl.set_upvalue(st.heap(), 1, owner);
    let top = st.top(); // owner at top - 1, upvalues below it
    let first = top - 1 - nup;
    for i in 0..nup {
        let v = st.get(first + i);
        cl.set_upvalue(st.heap(), FIRST_UPVALUE + i, v);
    }
    st.set_top(first);
    st.push(Value::cclosure(cl));
    st.check_gc();
    Ok(())
}

/// Push a userdata owning `obj` (a `RefCell<T>`). The caller reserved 3 slots.
pub(crate) fn new_userdata<T: 'static>(st: &mut State, obj: Rc<dyn Any>) -> Result<()> {
    let mt = type_metatable::<T>(st)?;
    let id = shared::insert(st, obj).ok_or_else(|| Error::api("not a rustynail state"))?;
    push_owner(st, id, mt)?;
    st.check_gc();
    Ok(())
}

/// The running Rust closure (the current frame's function), if it is one.
fn current_closure(st: &State) -> Result<rn_core::value::Gc<rn_core::func::CClosure>> {
    let fv = st.get(st.ci().r().func);
    match fv.as_cclosure() {
        Some(c) if c.nupvalues() >= FIRST_UPVALUE && udata_id(&c.upvalue(1)).is_some() => Ok(c),
        _ => Err(Error::api("not inside a Rust closure")),
    }
}

/// Upvalue `n` (1-based) of the running Rust closure.
pub(crate) fn upvalue(st: &State, n: usize) -> Result<Value> {
    let c = current_closure(st)?;
    if n == 0 || FIRST_UPVALUE + n > c.nupvalues() {
        return Err(Error::api(format!("invalid upvalue index {n}")));
    }
    Ok(c.upvalue(FIRST_UPVALUE + n - 1))
}

/// Set upvalue `n` (1-based) of the running Rust closure.
pub(crate) fn set_upvalue(st: &mut State, n: usize, v: Value) -> Result<()> {
    let c = current_closure(st)?;
    if n == 0 || FIRST_UPVALUE + n > c.nupvalues() {
        return Err(Error::api(format!("invalid upvalue index {n}")));
    }
    c.set_upvalue(st.heap(), FIRST_UPVALUE + n - 1, v);
    Ok(())
}

/// Raise `e` from a native function: returns the status to propagate.
fn raise(st: &mut State, e: Error) -> Status {
    match e.kind() {
        ErrorKind::InstructionLimit => {
            let s = st.run_error(LIMIT_MSG.as_bytes());
            return st.throw_base_level(s);
        }
        ErrorKind::Memory if e.seq == 0 => return Status::ErrMem,
        _ => {}
    }
    let stashed = if e.seq != 0 {
        shared::with(st, |sh| (sh.err_seq == e.seq).then_some(sh.err_slot))
            .flatten()
            .map(|slot| st.registry().get_int(slot))
    } else {
        None
    };
    let v = match stashed {
        Some(v) => v,
        None => match st.new_str_value(e.message()) {
            Ok(v) => v,
            Err(s) => return s,
        },
    };
    if v.raw_equal(&Value::string(st.g().memerrmsg)) {
        return Status::ErrMem;
    }
    st.throw_value(v)
}

/// The function of every Rust closure.
fn call_rust(st: &mut State) -> LuaResult<usize> {
    let func = st.ci().r().func;
    let cb = current_closure(st)
        .ok()
        .and_then(|c| c.upvalue(0).as_int())
        .and_then(|id| shared::get(st, id))
        .and_then(|o| o.downcast::<Callback>().ok());
    let Some(cb) = cb else {
        return Err(st.run_error(b"attempt to call a Rust function that was finalized"));
    };
    shared::with(st, |sh| sh.callback_depth += 1);
    let mut h = Handle {
        st: State::from_thread(st.as_gc()),
    };
    let r = catch_unwind(AssertUnwindSafe(|| (cb.0)(&mut h)));
    let limit = shared::with(st, |sh| {
        sh.callback_depth -= 1;
        sh.limit_hit
    })
    .unwrap_or(false);
    if limit {
        // keep unwinding to the outermost protected call, whatever the function returned
        let s = st.run_error(LIMIT_MSG.as_bytes());
        return Err(st.throw_base_level(s));
    }
    match r {
        Ok(Ok(n)) => {
            if n > st.top() - (func + 1) {
                Err(st.run_error(b"Rust function returned more results than values on its stack"))
            } else {
                Ok(n)
            }
        }
        Ok(Err(e)) if e.kind() == ErrorKind::Yield => {
            if e.nyield > st.top() - (func + 1) {
                return Err(st.run_error(b"not enough values to yield"));
            }
            st.yield_k(e.nyield, 0, None)
        }
        Ok(Err(e)) => Err(raise(st, e)),
        Err(p) => {
            let m = format!("Rust function panicked: {}", panic_message(&*p));
            Err(st.run_error(m.as_bytes()))
        }
    }
}

/// The count hook of [`crate::Lua::set_instruction_limit`]: an error that only the outermost
/// protected call of the thread can catch, so scripts cannot `pcall` their way past it.
pub(crate) fn limit_hook(st: &mut State, _event: i32, _line: i32) -> LuaResult<()> {
    shared::with(st, |sh| sh.limit_hit = true);
    let s = st.run_error(LIMIT_MSG.as_bytes());
    Err(st.throw_base_level(s))
}

/// Message handler adding a traceback (the standalone interpreter's `msghandler`).
pub(crate) fn traceback_handler(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let msg: Vec<u8> = if v.is_string() || v.is_number() {
        crate::stack::describe(&v)
    } else {
        if let Some(r) = st.call_meta(&v, b"__tostring")? {
            if r.is_string() {
                st.push(r);
                return Ok(1);
            }
        }
        crate::stack::describe(&v)
    };
    let tb = st.traceback(Some(&msg), 1);
    st.push_bytes(&tb)?;
    Ok(1)
}

/// `auxstatus` of `lcorolib.c`.
pub(crate) fn aux_status(st: &State, co: &State) -> ThreadStatus {
    if st.same_thread(co) {
        return ThreadStatus::Running;
    }
    match co.status() {
        Status::Yield => ThreadStatus::Suspended,
        Status::Ok => {
            if co.get_stack(0).is_some() {
                ThreadStatus::Normal
            } else if co.frame_top() == 0 {
                ThreadStatus::Dead
            } else {
                ThreadStatus::Suspended
            }
        }
        _ => ThreadStatus::Dead,
    }
}
