//! [`Stack`]: the operations of the C API on one Lua thread, as safe methods.
use crate::callback::{self, Callback};
use crate::error::{Error, ErrorKind, Result};
use crate::shared;
use crate::{GcMode, LoadMode, RegistryKey, Resume, ThreadStatus, Type};
use rn_core::arith;
use rn_core::number::{self, F2I};
use rn_core::state::{State, StkId};
use rn_core::status::{LuaResult, Status};
use rn_core::table::TableRef;
use rn_core::value::{LuaType, Value};
use std::any::Any;
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

/// Option for `nresults` of [`Stack::call`](trait.Stack.html#method.call)/[`Stack::pcall`](trait.Stack.html#method.pcall): keep all results (`LUA_MULTRET`).
pub const MULTRET: i32 = -1;

pub(crate) mod sealed {
    /// Implemented only by the crate's private handle type; keeps [`super::Stack`] unforgeable.
    pub trait Sealed {
        fn st(&mut self) -> &mut rn_core::state::State;
        fn st_ref(&self) -> &rn_core::state::State;
    }
}
use sealed::Sealed;

/// A borrowed view of one Lua thread's stack: every operation of the embedding API.
///
/// This trait cannot be implemented outside this crate, and it is only ever used as the trait
/// object `dyn Stack`: [`Lua`](crate::Lua) dereferences to the main thread's `dyn Stack`, and a
/// Rust function called from Lua receives `&mut dyn Stack` for the thread that called it. All
/// operations are inherent methods of `dyn Stack` (see below); they mirror the C API (`lua.h`,
/// `lauxlib.h`) one to one where possible, with the differences listed here.
///
/// # Stack indices
///
/// As in C, values are addressed by `i32` indices relative to the current function's frame:
/// `1` is the first value (the first argument inside a Rust function), `-1` the top. Index `0` and
/// indices outside the frame are *invalid*: query functions treat them as "no value" (like the C
/// API's acceptable indices) and every other function returns an [`ErrorKind::Api`] error.
/// Pseudo-indices are not used: the registry and upvalues have their own methods
/// ([`push_registry`](trait.Stack.html#method.push_registry), [`push_upvalue`](trait.Stack.html#method.push_upvalue), ...).
///
/// # Differences from the C API
///
/// - **Errors are values.** Every operation that can raise a Lua error (anything that may call a
///   metamethod, allocate, or run Lua code) runs protected and returns [`Result`]; there are no
///   unprotected calls and no panics/aborts. On error the operation's operands are popped, so the
///   stack is as before minus the operands. [`pcall`](trait.Stack.html#method.pcall) and [`resume`](trait.Stack.html#method.resume)
///   additionally push the error object, as in C.
/// - **Stack space is checked.** Every push grows the stack as needed (C requires
///   `lua_checkstack`); a stack that cannot grow (about 1,000,000 slots) gives an
///   [`ErrorKind::Api`] error.
/// - **No in-place conversions.** [`to_bytes`](trait.Stack.html#method.to_bytes) on a number returns its string form
///   without converting the stack slot (`lua_tolstring` would).
/// - **Rust functions are closures** ([`push_function`](trait.Stack.html#method.push_function)), not function
///   pointers; userdata hold typed Rust values ([`push_userdata`](trait.Stack.html#method.push_userdata)), not raw
///   memory blocks.
/// - Raising an error from a Rust function is `return Err(..)`, yielding is
///   `return s.yield_(n)` (`lua_error`/`lua_yield` would not return).
pub trait Stack: Sealed {}

/// The private handle behind every `dyn Stack`.
pub(crate) struct Handle {
    pub(crate) st: State,
}

impl Sealed for Handle {
    fn st(&mut self) -> &mut State {
        &mut self.st
    }
    fn st_ref(&self) -> &State {
        &self.st
    }
}
impl Stack for Handle {}

pub(crate) fn type_of_value(v: &Value) -> Type {
    match v.type_of() {
        LuaType::Nil => Type::Nil,
        LuaType::Boolean => Type::Boolean,
        LuaType::LightUserdata => Type::LightUserdata,
        LuaType::Number => Type::Number,
        LuaType::String => Type::String,
        LuaType::Table => Type::Table,
        LuaType::Function => Type::Function,
        LuaType::Userdata => Type::Userdata,
        LuaType::Thread => Type::Thread,
    }
}

/// The error message of an error object, as the standalone interpreter reports it.
pub(crate) fn describe(v: &Value) -> Vec<u8> {
    if let Some(s) = v.as_string() {
        s.as_bytes().to_vec()
    } else if let Some(n) = v.as_number() {
        number::tostring(n).as_bytes().to_vec()
    } else {
        let mut m = b"(error object is a ".to_vec();
        m.extend_from_slice(rn_core::auxlib::typename_of(v));
        m.extend_from_slice(b" value)");
        m
    }
}

fn kind_of(status: Status) -> ErrorKind {
    match status {
        Status::ErrSyntax => ErrorKind::Syntax,
        Status::ErrMem => ErrorKind::Memory,
        Status::ErrErr => ErrorKind::MessageHandler,
        _ => ErrorKind::Runtime,
    }
}

/// Turn the error object on top of the stack (raised with `status`) into an [`Error`]: the object
/// is stored in the registry slot (so returning the error from a Rust function re-raises it) and,
/// unless `keep`, popped.
pub(crate) fn catch(st: &mut State, status: Status, keep: bool) -> Error {
    let (limit, depth) =
        shared::with(st, |sh| (sh.limit_hit, sh.callback_depth)).unwrap_or_default();
    if limit && depth > 0 {
        // the instruction-limit error is unwinding to the outermost protected call (which runs
        // in the host); the stack and frames are not ours to touch until then
        return Error::new(ErrorKind::InstructionLimit, callback::LIMIT_MSG);
    }
    if limit {
        shared::with(st, |sh| sh.limit_hit = false);
    }
    let v = st.get(st.top() - 1);
    let mut kind = kind_of(status);
    if limit {
        kind = ErrorKind::InstructionLimit;
    }
    let mut e = Error::new(kind, describe(&v));
    if let Some((seq, slot)) = shared::with(st, |sh| {
        sh.err_seq += 1;
        (sh.err_seq, sh.err_slot)
    }) {
        let reg = st.registry();
        if st.raw_set_int(reg, slot, &v).is_ok() {
            e.seq = seq;
        }
    }
    if !keep {
        let t = st.top();
        st.set_top(t - 1);
    }
    e
}

fn api<T>(msg: &str) -> Result<T> {
    Err(Error::api(msg))
}

impl<'a> dyn Stack + 'a {
    // ---- internal helpers ---------------------------------------------------------------------

    fn base(&self) -> StkId {
        self.st_ref().ci().r().func + 1
    }

    /// Stack slot of a valid index.
    fn slot(&self, idx: i32) -> Result<StkId> {
        self.slot_opt(idx)
            .ok_or_else(|| Error::api(format!("invalid stack index {idx}")))
    }

    fn slot_opt(&self, idx: i32) -> Option<StkId> {
        let st = self.st_ref();
        let base = self.base();
        let top = st.top();
        if idx > 0 {
            let s = base + (idx as usize - 1);
            (s < top).then_some(s)
        } else if idx < 0 {
            let n = idx.unsigned_abs() as usize;
            (n <= top - base).then(|| top - n)
        } else {
            None
        }
    }

    fn value(&self, idx: i32) -> Result<Value> {
        let s = self.slot(idx)?;
        Ok(self.st_ref().get(s))
    }

    fn value_opt(&self, idx: i32) -> Option<Value> {
        self.slot_opt(idx).map(|s| self.st_ref().get(s))
    }

    fn table_at(&self, idx: i32) -> Result<TableRef> {
        match self.value(idx)?.as_table() {
            Some(t) => Ok(t),
            None => api("table expected"),
        }
    }

    /// Make room for `n` more values.
    fn room(&mut self, n: usize) -> Result<()> {
        if self.st().check_stack_api(n) {
            Ok(())
        } else {
            api("stack overflow (cannot grow the Lua stack)")
        }
    }

    /// At least `n` values in the frame.
    fn need(&self, n: usize) -> Result<()> {
        if self.top() as usize >= n {
            Ok(())
        } else {
            api("not enough values on the stack")
        }
    }

    fn host_level(&mut self) {
        // a fresh host-level operation: a stale instruction-limit flag (an error that a script
        // caught inside a coroutine) must not misclassify it
        let st = self.st();
        shared::with(st, |sh| {
            if sh.callback_depth == 0 {
                sh.limit_hit = false;
            }
        });
    }

    /// Run `f` protected. On error the top `consumed` values and everything `f` pushed are
    /// removed and the error object is stored for re-raising.
    fn protect(
        &mut self,
        consumed: usize,
        f: impl FnOnce(&mut State) -> LuaResult<()>,
    ) -> Result<()> {
        self.host_level();
        let st = self.st();
        let old_top = st.top();
        let status = st.pcall_with(f, old_top, 0);
        if status == Status::Ok {
            return Ok(());
        }
        let e = catch(st, status, false);
        if e.kind() != ErrorKind::InstructionLimit || st.top() >= old_top {
            let t = st.top().0.min(old_top.0);
            st.set_top(StkId(t - consumed.min(t)));
        }
        Err(e)
    }

    fn push_raw(&mut self, v: Value) -> Result<()> {
        self.room(1)?;
        self.st().push(v);
        Ok(())
    }

    // ---- basic stack manipulation -------------------------------------------------------------

    /// Number of values in the current frame; also the index of the top value (`lua_gettop`).
    pub fn top(&self) -> i32 {
        let st = self.st_ref();
        (st.top() - self.base()) as i32
    }

    /// Set the number of values in the frame to `n`, filling with `nil` or dropping values from the
    /// top (`lua_settop` with a non-negative index).
    pub fn set_top(&mut self, n: usize) -> Result<()> {
        let cur = self.top() as usize;
        if n > cur {
            self.room(n - cur)?;
            for _ in cur..n {
                self.st().push(Value::NIL);
            }
        } else {
            let st = self.st();
            let t = st.top();
            st.set_top(t - (cur - n));
        }
        Ok(())
    }

    /// Pop `n` values (`lua_pop`); popping more values than the frame holds is an error.
    pub fn pop(&mut self, n: usize) -> Result<()> {
        self.need(n)?;
        let st = self.st();
        let t = st.top();
        st.set_top(t - n);
        Ok(())
    }

    /// Convert an index into an absolute (positive) one (`lua_absindex`); invalid indices are
    /// returned unchanged.
    pub fn abs_index(&self, idx: i32) -> i32 {
        match self.slot_opt(idx) {
            Some(s) => (s - self.base()) as i32 + 1,
            None => idx,
        }
    }

    /// Ensure room for `n` more values (`lua_checkstack`). Pushes check by themselves; this is
    /// for reserving space in advance.
    pub fn check_stack(&mut self, n: usize) -> Result<()> {
        self.room(n)
    }

    /// Push a copy of the value at `idx` (`lua_pushvalue`).
    pub fn push_value(&mut self, idx: i32) -> Result<()> {
        let v = self.value(idx)?;
        self.push_raw(v)
    }

    /// Copy the value at `from` into the slot `to` (`lua_copy`).
    pub fn copy(&mut self, from: i32, to: i32) -> Result<()> {
        let v = self.value(from)?;
        let s = self.slot(to)?;
        self.st().set(s, v);
        Ok(())
    }

    /// Rotate the values between `idx` and the top `n` positions towards the top (negative `n`:
    /// towards the bottom) (`lua_rotate`).
    pub fn rotate(&mut self, idx: i32, n: i32) -> Result<()> {
        let p = self.slot(idx)?.0;
        let st = self.st();
        let t = st.top().0 - 1;
        let len = t + 1 - p;
        if n.unsigned_abs() as usize > len {
            return api("rotation larger than the segment");
        }
        let m = if n >= 0 {
            t - n as usize
        } else {
            p + n.unsigned_abs() as usize - 1
        };
        fn reverse(st: &mut State, mut from: usize, mut to: usize) {
            while from < to {
                let a = st.get(StkId(from));
                let b = st.get(StkId(to));
                st.set(StkId(from), b);
                st.set(StkId(to), a);
                from += 1;
                to -= 1;
            }
        }
        reverse(st, p, m);
        reverse(st, m + 1, t);
        reverse(st, p, t);
        Ok(())
    }

    /// Move the top value into position `idx`, shifting the values above it up (`lua_insert`).
    pub fn insert(&mut self, idx: i32) -> Result<()> {
        self.rotate(idx, 1)
    }

    /// Remove the value at `idx`, shifting the values above it down (`lua_remove`).
    pub fn remove(&mut self, idx: i32) -> Result<()> {
        self.rotate(idx, -1)?;
        self.pop(1)
    }

    /// Pop the top value into position `idx` (`lua_replace`).
    pub fn replace(&mut self, idx: i32) -> Result<()> {
        self.need(1)?;
        self.copy(-1, idx)?;
        self.pop(1)
    }

    // ---- access functions ---------------------------------------------------------------------

    /// Type of the value at `idx`; `None` for an invalid index (`lua_type`'s `LUA_TNONE`).
    pub fn type_of(&self, idx: i32) -> Option<Type> {
        self.value_opt(idx).map(|v| type_of_value(&v))
    }

    /// Name of the type of the value at `idx` (`"no value"` for an invalid index).
    pub fn type_name(&self, idx: i32) -> &'static str {
        self.type_of(idx).map_or("no value", Type::name)
    }

    /// Is the value at `idx` `nil`?
    pub fn is_nil(&self, idx: i32) -> bool {
        self.type_of(idx) == Some(Type::Nil)
    }

    /// Is `idx` invalid or its value `nil` (`lua_isnoneornil`)?
    pub fn is_none_or_nil(&self, idx: i32) -> bool {
        matches!(self.type_of(idx), None | Some(Type::Nil))
    }

    /// Is the value an integer (the subtype, not a convertible value) (`lua_isinteger`)?
    pub fn is_integer(&self, idx: i32) -> bool {
        self.value_opt(idx).is_some_and(|v| v.is_int())
    }

    /// Is the value a number or a string convertible to one (`lua_isnumber`)?
    pub fn is_number(&self, idx: i32) -> bool {
        self.to_number(idx).is_some()
    }

    /// Is the value a string or a number (`lua_isstring`)?
    pub fn is_string(&self, idx: i32) -> bool {
        self.value_opt(idx)
            .is_some_and(|v| v.is_string() || v.is_number())
    }

    /// Is the value a table?
    pub fn is_table(&self, idx: i32) -> bool {
        self.type_of(idx) == Some(Type::Table)
    }

    /// Is the value a function (Lua or Rust)?
    pub fn is_function(&self, idx: i32) -> bool {
        self.type_of(idx) == Some(Type::Function)
    }

    /// The value as a boolean: `false` only for `nil`, `false` and invalid indices
    /// (`lua_toboolean`).
    pub fn to_boolean(&self, idx: i32) -> bool {
        self.value_opt(idx).is_some_and(|v| v.is_truthy())
    }

    /// The value as an integer: integers, floats with an exact integer value, and strings
    /// convertible to such numbers (`lua_tointegerx`).
    pub fn to_integer(&self, idx: i32) -> Option<i64> {
        self.value_opt(idx)
            .and_then(|v| arith::to_integer(&v, F2I::Eq))
    }

    /// The value as a float: numbers and strings convertible to numbers (`lua_tonumberx`).
    pub fn to_number(&self, idx: i32) -> Option<f64> {
        self.value_opt(idx).and_then(|v| arith::to_float(&v))
    }

    /// A copy of the bytes of a string, or the string form of a number (`lua_tolstring`, but the
    /// slot is not converted).
    pub fn to_bytes(&self, idx: i32) -> Option<Vec<u8>> {
        self.with_bytes(idx, <[u8]>::to_vec)
    }

    /// [`to_bytes`](trait.Stack.html#method.to_bytes), converted lossily to UTF-8.
    pub fn to_string_lossy(&self, idx: i32) -> Option<String> {
        self.with_bytes(idx, |b| String::from_utf8_lossy(b).into_owned())
    }

    /// Call `f` with the bytes of a string (or the string form of a number) without copying them.
    pub fn with_bytes<R>(&self, idx: i32, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let v = self.value_opt(idx)?;
        if let Some(s) = v.as_string() {
            Some(f(s.as_bytes()))
        } else {
            v.as_number().map(|n| f(number::tostring(n).as_bytes()))
        }
    }

    /// The pointer of a light userdata (`lua_touserdata` for light userdata).
    pub fn to_light_userdata(&self, idx: i32) -> Option<*mut c_void> {
        self.value_opt(idx).and_then(|v| v.as_light_userdata())
    }

    /// Raw length (`lua_rawlen`): string length, userdata payload size (0 for Rust userdata),
    /// table border without `__len`; 0 for other values.
    pub fn raw_len(&self, idx: i32) -> u64 {
        let Some(v) = self.value_opt(idx) else {
            return 0;
        };
        if let Some(s) = v.as_string() {
            s.as_bytes().len() as u64
        } else if let Some(u) = v.as_userdata() {
            u.len() as u64
        } else if v.is_table() {
            self.st_ref().raw_len(&v)
        } else {
            0
        }
    }

    /// Primitive equality, without metamethods (`lua_rawequal`); `false` for invalid indices.
    pub fn raw_equal(&self, a: i32, b: i32) -> bool {
        match (self.value_opt(a), self.value_opt(b)) {
            (Some(x), Some(y)) => x.raw_equal(&y),
            _ => false,
        }
    }

    /// `==` with metamethods (`lua_compare(L, a, b, LUA_OPEQ)`).
    pub fn equal(&mut self, a: i32, b: i32) -> Result<bool> {
        self.compare(a, b, 0)
    }

    /// `<` with metamethods (`LUA_OPLT`).
    pub fn less_than(&mut self, a: i32, b: i32) -> Result<bool> {
        self.compare(a, b, 1)
    }

    /// `<=` with metamethods (`LUA_OPLE`).
    pub fn less_equal(&mut self, a: i32, b: i32) -> Result<bool> {
        self.compare(a, b, 2)
    }

    fn compare(&mut self, a: i32, b: i32, op: u8) -> Result<bool> {
        let (x, y) = (self.value(a)?, self.value(b)?);
        let mut out = false;
        self.protect(0, |st| {
            out = match op {
                0 => st.equals(x, y)?,
                1 => st.compare_lt_le(x, y, false)?,
                _ => st.compare_lt_le(x, y, true)?,
            };
            Ok(())
        })?;
        Ok(out)
    }

    /// The value converted to a string like `tostring` does (`__tostring`, `__name`; `luaL_tolstring`,
    /// but nothing is left on the stack).
    pub fn tostring(&mut self, idx: i32) -> Result<Vec<u8>> {
        let v = self.value(idx)?;
        self.room(2)?;
        let mut out = Vec::new();
        self.protect(0, |st| {
            let s = st.tolstring(v)?;
            out = s.get().as_bytes().to_vec();
            let t = st.top();
            st.set_top(t - 1);
            Ok(())
        })?;
        Ok(out)
    }

    /// Push the length of the value at `idx`, honouring `__len` (`lua_len`).
    pub fn len(&mut self, idx: i32) -> Result<()> {
        let v = self.value(idx)?;
        self.room(1)?;
        self.protect(0, |st| {
            let l = st.len_value(v)?;
            st.push(l);
            Ok(())
        })
    }

    /// The length of the value at `idx` as an integer, honouring `__len` (`luaL_len`).
    pub fn len_integer(&mut self, idx: i32) -> Result<i64> {
        let v = self.value(idx)?;
        self.room(1)?;
        let mut out = 0;
        self.protect(0, |st| {
            out = st.len_int(v)?;
            Ok(())
        })?;
        Ok(out)
    }

    /// Pop `n` values and push their concatenation, honouring `__concat` (`lua_concat`).
    pub fn concat(&mut self, n: usize) -> Result<()> {
        self.need(n)?;
        self.room(1)?;
        if n == 0 {
            return self.push_bytes(b"");
        }
        self.protect(n, |st| st.concat(n))?;
        self.st().check_gc();
        Ok(())
    }

    // ---- push functions -----------------------------------------------------------------------

    /// Push `nil`.
    pub fn push_nil(&mut self) -> Result<()> {
        self.push_raw(Value::NIL)
    }

    /// Push a boolean.
    pub fn push_boolean(&mut self, b: bool) -> Result<()> {
        self.push_raw(Value::boolean(b))
    }

    /// Push an integer.
    pub fn push_integer(&mut self, i: i64) -> Result<()> {
        self.push_raw(Value::int(i))
    }

    /// Push a float.
    pub fn push_number(&mut self, n: f64) -> Result<()> {
        self.push_raw(Value::float(n))
    }

    /// Push a string given as bytes (`lua_pushlstring`; Lua strings may hold any bytes).
    pub fn push_bytes(&mut self, b: &[u8]) -> Result<()> {
        self.room(1)?;
        let st = self.st();
        match st.new_str_value(b) {
            Ok(v) => st.push(v),
            Err(_) => return Err(Error::new(ErrorKind::Memory, "not enough memory")),
        }
        st.check_gc();
        Ok(())
    }

    /// Push a string.
    pub fn push_str(&mut self, s: &str) -> Result<()> {
        self.push_bytes(s.as_bytes())
    }

    /// Push a light userdata: a bare pointer value that Lua only stores and compares
    /// (`lua_pushlightuserdata`). It is never dereferenced by this crate.
    pub fn push_light_userdata(&mut self, p: *mut c_void) -> Result<()> {
        self.push_raw(Value::light_userdata(p))
    }

    /// Push the globals table.
    pub fn push_globals(&mut self) -> Result<()> {
        let g = self.st_ref().globals();
        self.push_raw(Value::table(g))
    }

    /// Push the registry table (`lua_pushvalue(L, LUA_REGISTRYINDEX)`). Integer keys are used by
    /// [`reference`](trait.Stack.html#method.reference) and by the state itself; use string keys prefixed with your
    /// crate's name for your own entries.
    pub fn push_registry(&mut self) -> Result<()> {
        let r = self.st_ref().registry();
        self.push_raw(Value::table(r))
    }

    /// Push the thread this stack belongs to; returns whether it is the main thread
    /// (`lua_pushthread`).
    pub fn push_thread(&mut self) -> Result<bool> {
        let v = Value::thread(self.st_ref().as_gc());
        self.push_raw(v)?;
        Ok(self.st_ref().is_main_thread())
    }

    /// Push a Rust function. When Lua calls it, it receives the caller's stack with the arguments
    /// at indices `1..=top()`, pushes its results and returns how many there are (the top
    /// `n` values are the results). Returning `Err(e)` raises `e` as a Lua error; a panic is
    /// caught and raised as a Lua error too.
    ///
    /// The closure is owned by Lua and dropped when the function value is collected (or when the
    /// state closes).
    pub fn push_function<F>(&mut self, f: F) -> Result<()>
    where
        F: Fn(&mut dyn Stack) -> Result<usize> + 'static,
    {
        self.push_closure(f, 0)
    }

    /// Push a Rust function with `nupvalues` Lua upvalues, popped from the stack (the deepest one
    /// is upvalue 1; `lua_pushcclosure`). The function reads and writes them with
    /// [`push_upvalue`](trait.Stack.html#method.push_upvalue) and [`replace_upvalue`](trait.Stack.html#method.replace_upvalue).
    pub fn push_closure<F>(&mut self, f: F, nupvalues: usize) -> Result<()>
    where
        F: Fn(&mut dyn Stack) -> Result<usize> + 'static,
    {
        self.need(nupvalues)?;
        if nupvalues > 253 {
            return api("too many upvalues (limit is 253)");
        }
        self.room(2)?;
        let obj: Rc<dyn Any> = Rc::new(Callback(Box::new(f)));
        callback::new_closure(self.st(), obj, nupvalues)
    }

    // ---- tables and fields --------------------------------------------------------------------

    /// Push a new empty table (`lua_newtable`).
    pub fn new_table(&mut self) -> Result<()> {
        self.create_table(0, 0)
    }

    /// Push a new table with space preallocated for `narr` array and `nrec` hash elements
    /// (`lua_createtable`).
    pub fn create_table(&mut self, narr: usize, nrec: usize) -> Result<()> {
        self.room(1)?;
        let st = self.st();
        match st.new_table_sized(narr, nrec) {
            Ok(t) => st.push(Value::table(t)),
            Err(_) => return Err(Error::new(ErrorKind::Memory, "not enough memory")),
        }
        st.check_gc();
        Ok(())
    }

    /// Push `t[k]` where `t` is the value at `idx` and `k` the value on top, which is replaced by
    /// the result (`lua_gettable`, may call `__index`). Returns the result's type.
    pub fn get_table(&mut self, idx: i32) -> Result<Type> {
        let t = self.value(idx)?;
        self.need(1)?;
        self.protect(1, |st| {
            let top = st.top();
            let key = st.get(top - 1);
            let v = st.index(t, key)?;
            st.set(top - 1, v);
            Ok(())
        })?;
        Ok(self.top_type())
    }

    /// Push `t[name]` where `t` is the value at `idx` (`lua_getfield`).
    pub fn get_field(&mut self, idx: i32, name: impl AsRef<[u8]>) -> Result<Type> {
        let t = self.value(idx)?;
        self.room(1)?;
        let k = name.as_ref();
        self.protect(0, |st| {
            let v = st.index_str(t, k)?;
            st.push(v);
            Ok(())
        })?;
        Ok(self.top_type())
    }

    /// Push `t[i]` where `t` is the value at `idx` (`lua_geti`).
    pub fn get_i(&mut self, idx: i32, i: i64) -> Result<Type> {
        let t = self.value(idx)?;
        self.room(1)?;
        self.protect(0, |st| {
            let v = st.index_int(t, i)?;
            st.push(v);
            Ok(())
        })?;
        Ok(self.top_type())
    }

    /// Push the global `name` (`lua_getglobal`).
    pub fn get_global(&mut self, name: impl AsRef<[u8]>) -> Result<Type> {
        let g = Value::table(self.st_ref().globals());
        self.room(1)?;
        let k = name.as_ref();
        self.protect(0, |st| {
            let v = st.index_str(g, k)?;
            st.push(v);
            Ok(())
        })?;
        Ok(self.top_type())
    }

    /// `t[k] = v` where `t` is the value at `idx`, `v` the top value and `k` the one below it;
    /// pops both (`lua_settable`, may call `__newindex`).
    pub fn set_table(&mut self, idx: i32) -> Result<()> {
        let t = self.value(idx)?;
        self.need(2)?;
        self.protect(2, |st| {
            let top = st.top();
            let (k, v) = (st.get(top - 2), st.get(top - 1));
            st.set_index(t, k, v)?;
            st.set_top(top - 2);
            Ok(())
        })
    }

    /// `t[name] = v` where `t` is the value at `idx` and `v` the top value, which is popped
    /// (`lua_setfield`).
    pub fn set_field(&mut self, idx: i32, name: impl AsRef<[u8]>) -> Result<()> {
        let t = self.value(idx)?;
        self.need(1)?;
        let k = name.as_ref();
        self.protect(1, |st| {
            let top = st.top();
            let v = st.get(top - 1);
            st.set_index_str(t, k, v)?;
            st.set_top(top - 1);
            Ok(())
        })
    }

    /// `t[i] = v` where `t` is the value at `idx` and `v` the top value, which is popped
    /// (`lua_seti`).
    pub fn set_i(&mut self, idx: i32, i: i64) -> Result<()> {
        let t = self.value(idx)?;
        self.need(1)?;
        self.protect(1, |st| {
            let top = st.top();
            let v = st.get(top - 1);
            st.set_index_int(t, i, v)?;
            st.set_top(top - 1);
            Ok(())
        })
    }

    /// Pop the top value into the global `name` (`lua_setglobal`).
    pub fn set_global(&mut self, name: impl AsRef<[u8]>) -> Result<()> {
        let g = Value::table(self.st_ref().globals());
        self.need(1)?;
        let k = name.as_ref();
        self.protect(1, |st| {
            let top = st.top();
            let v = st.get(top - 1);
            st.set_index_str(g, k, v)?;
            st.set_top(top - 1);
            Ok(())
        })
    }

    /// Like [`get_table`](trait.Stack.html#method.get_table) without metamethods; the value at `idx` must be a table
    /// (`lua_rawget`).
    pub fn raw_get(&mut self, idx: i32) -> Result<Type> {
        let t = self.table_at(idx)?;
        self.need(1)?;
        let st = self.st();
        let top = st.top();
        let v = t.get(&st.get(top - 1));
        st.set(top - 1, v);
        Ok(type_of_value(&v))
    }

    /// Push `t[i]` without metamethods; the value at `idx` must be a table (`lua_rawgeti`).
    pub fn raw_get_i(&mut self, idx: i32, i: i64) -> Result<Type> {
        let t = self.table_at(idx)?;
        let v = t.get_int(i);
        self.push_raw(v)?;
        Ok(type_of_value(&v))
    }

    /// Like [`set_table`](trait.Stack.html#method.set_table) without metamethods (`lua_rawset`).
    pub fn raw_set(&mut self, idx: i32) -> Result<()> {
        let t = self.table_at(idx)?;
        self.need(2)?;
        self.protect(2, |st| {
            let top = st.top();
            let (k, v) = (st.get(top - 2), st.get(top - 1));
            st.raw_set(t, &k, &v)?;
            st.set_top(top - 2);
            Ok(())
        })
    }

    /// Like [`set_i`](trait.Stack.html#method.set_i) without metamethods (`lua_rawseti`).
    pub fn raw_set_i(&mut self, idx: i32, i: i64) -> Result<()> {
        let t = self.table_at(idx)?;
        self.need(1)?;
        self.protect(1, |st| {
            let top = st.top();
            let v = st.get(top - 1);
            st.raw_set_int(t, i, &v)?;
            st.set_top(top - 1);
            Ok(())
        })
    }

    /// Table traversal (`lua_next`): pops a key and pushes the next key-value pair of the table
    /// at `idx`, returning `true`, or pops the key and returns `false` at the end. Start with a
    /// `nil` key. As in C, do not modify the table's keys during the traversal (assigning to
    /// existing fields, including `nil`, is allowed).
    ///
    /// ```
    /// # fn main() -> rustynail::Result<()> {
    /// let mut lua = rustynail::Lua::new()?;
    /// lua.do_string("t = {10, 20, 30}")?;
    /// lua.get_global("t")?;
    /// lua.push_nil()?;
    /// let mut sum = 0;
    /// while lua.next(-2)? {
    ///     sum += lua.to_integer(-1).unwrap();
    ///     lua.pop(1)?; // keep the key for the next iteration
    /// }
    /// assert_eq!(sum, 60);
    /// # Ok(()) }
    /// ```
    pub fn next(&mut self, idx: i32) -> Result<bool> {
        let t = self.table_at(idx)?;
        self.need(1)?;
        self.room(1)?;
        let st = self.st();
        let top = st.top();
        let key = st.get(top - 1);
        match t.next(&key) {
            Ok(Some((k, v))) => {
                st.set(top - 1, k);
                st.push(v);
                Ok(true)
            }
            Ok(None) => {
                st.set_top(top - 1);
                Ok(false)
            }
            Err(e) => {
                self.protect(1, |st| Err(st.table_error(e)))?;
                Ok(false)
            }
        }
    }

    /// Push the metatable of the value at `idx` and return `true`, or push nothing and return
    /// `false` if it has none (`lua_getmetatable`).
    pub fn get_metatable(&mut self, idx: i32) -> Result<bool> {
        let v = self.value(idx)?;
        match self.st_ref().metatable_of(&v) {
            Some(mt) => {
                self.push_raw(Value::table(mt))?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Pop a table (or `nil`) and set it as the metatable of the value at `idx`
    /// (`lua_setmetatable`). For values other than tables and userdata this sets the metatable
    /// shared by all values of that type.
    ///
    /// Userdata created by [`push_userdata`](trait.Stack.html#method.push_userdata) must keep a metatable whose
    /// `__gc` is the one installed by this crate, or their Rust value is only dropped when the
    /// state closes; changing their metatable is refused.
    pub fn set_metatable(&mut self, idx: i32) -> Result<()> {
        let obj = self.value(idx)?;
        self.need(1)?;
        let mtv = self.value(-1)?;
        let mt = match mtv.as_table() {
            Some(t) => Some(t),
            None if mtv.is_nil() => None,
            None => return api("table or nil expected as metatable"),
        };
        if callback::is_rust_userdata(self.st_ref(), &obj) {
            return api("cannot change the metatable of a Rust userdata");
        }
        let st = self.st();
        st.set_metatable(&obj, mt);
        let t = st.top();
        st.set_top(t - 1);
        Ok(())
    }

    fn top_type(&self) -> Type {
        let st = self.st_ref();
        type_of_value(&st.get(st.top() - 1))
    }

    // ---- calls --------------------------------------------------------------------------------

    fn call_setup(&mut self, nargs: usize, nresults: i32) -> Result<StkId> {
        if nresults < MULTRET {
            return api("invalid number of results");
        }
        self.need(nargs + 1)?;
        if nresults > nargs as i32 {
            self.room(nresults as usize - nargs)?;
        }
        Ok(self.st_ref().top() - (nargs + 1))
    }

    /// Call the function below the top `nargs` values with those arguments, replacing function
    /// and arguments by `nresults` results ([`MULTRET`]: all of them) (`lua_call`). Always
    /// protected: on error, function and arguments are popped and the error is returned.
    pub fn call(&mut self, nargs: usize, nresults: i32) -> Result<()> {
        let func = self.call_setup(nargs, nresults)?;
        self.host_level();
        let st = self.st();
        let status = st.pcall(func, nresults, 0);
        if status == Status::Ok {
            Ok(())
        } else {
            Err(catch(st, status, false))
        }
    }

    /// Protected call (`lua_pcall`): like [`call`](trait.Stack.html#method.call), but on error the error object is
    /// left on the stack in place of function and arguments (also returned as [`Error`]).
    /// `msgh` is the stack index of a message handler, or `0` for none; the handler is called
    /// with the error object and returns the new error object (see
    /// [`push_traceback_handler`](trait.Stack.html#method.push_traceback_handler)).
    pub fn pcall(&mut self, nargs: usize, nresults: i32, msgh: i32) -> Result<()> {
        let ef = if msgh == 0 { 0 } else { self.slot(msgh)?.0 };
        let func = self.call_setup(nargs, nresults)?;
        if ef >= func.0 && msgh != 0 {
            return api("the message handler must be below the function");
        }
        self.host_level();
        let st = self.st();
        let status = st.pcall(func, nresults, ef);
        if status == Status::Ok {
            Ok(())
        } else {
            Err(catch(st, status, true))
        }
    }

    /// Push a message handler for [`pcall`](trait.Stack.html#method.pcall) that appends a stack traceback to the
    /// error message, like the standalone interpreter's.
    pub fn push_traceback_handler(&mut self) -> Result<()> {
        self.push_raw(Value::native(callback::traceback_handler))
    }

    /// The stack traceback of this thread, starting at `level` (0 = the running function,
    /// 1 = its caller, ...), headed by `msg` if given (`luaL_traceback`).
    pub fn traceback(&mut self, msg: Option<&[u8]>, level: i32) -> Vec<u8> {
        self.st().traceback(msg, level)
    }

    /// Pop the top value and return an [`Error`] that, returned from a Rust function, raises that
    /// value as the error object (the counterpart of `lua_error`; tables and other values are
    /// preserved).
    pub fn error(&mut self) -> Error {
        if self.top() == 0 {
            return Error::api("no error object on the stack");
        }
        catch(self.st(), Status::ErrRun, false)
    }

    // ---- load and run -------------------------------------------------------------------------

    /// Compile a chunk and push it as a function (`luaL_loadbufferx`). `chunk_name` is used in
    /// messages and debug information, verbatim: by convention `"=name"` is shown as `name`,
    /// `"@file.lua"` as a file name, anything else as `[string "..."]`.
    pub fn load(&mut self, chunk: &[u8], chunk_name: &str, mode: LoadMode) -> Result<()> {
        self.room(1)?;
        self.host_level();
        let st = self.st();
        let status = st.load_chunk(chunk, chunk_name.as_bytes(), Some(mode.as_bytes()));
        if status == Status::Ok {
            st.check_gc();
            Ok(())
        } else {
            Err(catch(st, status, false))
        }
    }

    /// Compile a source string and push it as a function (`luaL_loadstring`: text only, the chunk
    /// name is the source itself).
    pub fn load_string(&mut self, src: &str) -> Result<()> {
        self.room(1)?;
        self.host_level();
        let st = self.st();
        let status = st.load_string(src.as_bytes());
        if status == Status::Ok {
            st.check_gc();
            Ok(())
        } else {
            Err(catch(st, status, false))
        }
    }

    /// Compile a file and push it as a function (`luaL_loadfilex`; a first line starting with
    /// `#` is skipped). Errors opening or reading the file have kind [`ErrorKind::File`].
    pub fn load_file(&mut self, path: impl AsRef<std::path::Path>, mode: LoadMode) -> Result<()> {
        use std::os::unix::ffi::OsStrExt;
        self.room(1)?;
        self.host_level();
        let name = path.as_ref().as_os_str().as_bytes();
        let st = self.st();
        let status = st.load_filex(Some(name), Some(mode.as_bytes()));
        if status == Status::Ok {
            st.check_gc();
            return Ok(());
        }
        let msg = st.get(st.top() - 1);
        let file_error = msg.as_string().is_some_and(|s| {
            let b = s.as_bytes();
            (b.starts_with(b"cannot open ") || b.starts_with(b"cannot read "))
                && b.get(12..12 + name.len()) == Some(name)
        });
        let e = catch(st, status, false);
        if file_error && e.kind() == ErrorKind::Syntax {
            let mut f = Error::new(ErrorKind::File, e.message().to_vec());
            f.seq = e.seq;
            return Err(f);
        }
        Err(e)
    }

    /// Run a source string (`luaL_dostring`): load it and call it with no arguments, leaving all
    /// its results on the stack. Returns the number of results.
    pub fn do_string(&mut self, src: &str) -> Result<usize> {
        let before = self.top();
        self.load_string(src)?;
        self.call(0, MULTRET)?;
        Ok((self.top() - before) as usize)
    }

    /// Run a file (`luaL_dofile`), leaving all its results on the stack. Returns the number of
    /// results.
    pub fn do_file(&mut self, path: impl AsRef<std::path::Path>) -> Result<usize> {
        let before = self.top();
        self.load_file(path, LoadMode::TextOrBinary)?;
        self.call(0, MULTRET)?;
        Ok((self.top() - before) as usize)
    }

    // ---- registry references ------------------------------------------------------------------

    /// Pop the top value and store it in the registry, returning a key for it (`luaL_ref` on the
    /// registry). The value stays alive until [`unreference`](trait.Stack.html#method.unreference). Popping `nil`
    /// gives a key that always pushes `nil`.
    pub fn reference(&mut self) -> Result<RegistryKey> {
        self.need(1)?;
        let reg = self.st_ref().registry();
        let mut r = 0;
        self.protect(1, |st| {
            let v = st.get(st.top() - 1);
            r = st.ref_(reg, v)?;
            let t = st.top();
            st.set_top(t - 1);
            Ok(())
        })?;
        Ok(RegistryKey(r))
    }

    /// Push the value stored under `key` (`lua_rawgeti(L, LUA_REGISTRYINDEX, ref)`).
    pub fn push_reference(&mut self, key: &RegistryKey) -> Result<()> {
        let v = self.st_ref().registry().get_int(i64::from(key.0));
        self.push_raw(v)
    }

    /// Release a registry reference (`luaL_unref`).
    pub fn unreference(&mut self, key: RegistryKey) -> Result<()> {
        let reg = self.st_ref().registry();
        self.protect(0, |st| st.unref(reg, key.0))
    }

    // ---- upvalues of Rust functions -----------------------------------------------------------

    /// Inside a Rust function created by [`push_closure`](trait.Stack.html#method.push_closure): push its upvalue
    /// `n` (1-based).
    pub fn push_upvalue(&mut self, n: usize) -> Result<()> {
        let v = callback::upvalue(self.st_ref(), n)?;
        self.push_raw(v)
    }

    /// Inside a Rust function created by [`push_closure`](trait.Stack.html#method.push_closure): pop the top value
    /// into its upvalue `n` (1-based).
    pub fn replace_upvalue(&mut self, n: usize) -> Result<()> {
        let v = self.value(-1)?;
        callback::set_upvalue(self.st(), n, v)?;
        self.pop(1)
    }

    // ---- userdata -----------------------------------------------------------------------------

    /// Push a full userdata owning `value`. Its metatable is the one of type `T` (see
    /// [`push_userdata_metatable`](trait.Stack.html#method.push_userdata_metatable)), whose `__gc` drops the value
    /// when Lua collects the userdata (or when the state closes).
    pub fn push_userdata<T: 'static>(&mut self, value: T) -> Result<()> {
        self.room(3)?;
        let obj: Rc<dyn Any> = Rc::new(RefCell::new(value));
        callback::new_userdata::<T>(self.st(), obj)
    }

    /// Push the metatable shared by all userdata of type `T`, creating it on first use with
    /// `__gc` (drops the Rust value) and `__name` (the Rust type name, used by `tostring`). Add
    /// methods to it (for example an `__index` table) before or after creating userdata.
    pub fn push_userdata_metatable<T: 'static>(&mut self) -> Result<()> {
        self.room(3)?;
        let mt = callback::type_metatable::<T>(self.st())?;
        self.push_raw(Value::table(mt))
    }

    /// Is the value at `idx` a userdata holding a `T` (created by
    /// [`push_userdata`](trait.Stack.html#method.push_userdata) and not yet finalized)?
    pub fn is_userdata<T: 'static>(&self, idx: i32) -> bool {
        self.value_opt(idx)
            .and_then(|v| callback::userdata_object(self.st_ref(), &v))
            .is_some_and(|o| o.downcast::<RefCell<T>>().is_ok())
    }

    /// Call `f` with a shared reference to the `T` held by the userdata at `idx`.
    pub fn with_userdata<T: 'static, R>(&self, idx: i32, f: impl FnOnce(&T) -> R) -> Result<R> {
        let cell = self.userdata_cell::<T>(idx)?;
        let b = cell
            .try_borrow()
            .map_err(|_| Error::api("userdata is mutably borrowed"))?;
        Ok(f(&b))
    }

    /// Call `f` with a mutable reference to the `T` held by the userdata at `idx`.
    pub fn with_userdata_mut<T: 'static, R>(
        &self,
        idx: i32,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R> {
        let cell = self.userdata_cell::<T>(idx)?;
        let mut b = cell
            .try_borrow_mut()
            .map_err(|_| Error::api("userdata is already borrowed"))?;
        Ok(f(&mut b))
    }

    fn userdata_cell<T: 'static>(&self, idx: i32) -> Result<Rc<RefCell<T>>> {
        let v = self.value(idx)?;
        if !v.is_userdata() {
            return Err(Error::api(format!(
                "userdata expected, got {}",
                type_of_value(&v).name()
            )));
        }
        let o = callback::userdata_object(self.st_ref(), &v)
            .ok_or_else(|| Error::api("not a Rust userdata (or already finalized)"))?;
        o.downcast::<RefCell<T>>().map_err(|_| {
            Error::api(format!(
                "userdata does not hold a {}",
                std::any::type_name::<T>()
            ))
        })
    }

    // ---- coroutines ---------------------------------------------------------------------------

    /// Pop a function and push a new coroutine (thread) that will run it
    /// (`coroutine.create`; `lua_newthread` + `lua_xmove` of the function).
    pub fn create_coroutine(&mut self) -> Result<()> {
        let f = self.value(-1)?;
        if !f.is_function() {
            return api("function expected");
        }
        self.room(1)?;
        let st = self.st();
        let th = match st.new_thread() {
            Ok(t) => t,
            Err(_) => return Err(Error::new(ErrorKind::Memory, "not enough memory")),
        };
        let mut co = State::from_thread(th);
        co.push(f); // a new thread has `LUA_MINSTACK` free slots
        let top = st.top();
        let tv = st.get(top - 1);
        st.set(top - 2, tv);
        st.set_top(top - 1);
        Ok(())
    }

    /// Resume the coroutine at `idx` (`lua_resume`, with `coroutine.resume`'s value transfer):
    /// pops `nargs` arguments, passes them to the coroutine (the body's arguments on the first
    /// resume, the results of `yield` afterwards) and pushes the values it yields or returns. On
    /// error the coroutine is dead and the error object is pushed (and returned).
    pub fn resume(&mut self, idx: i32, nargs: usize) -> Result<Resume> {
        let th = match self.value(idx)?.as_thread() {
            Some(t) => t,
            None => return api("thread expected"),
        };
        self.need(nargs + 1)?;
        if self.slot(idx)? >= self.st_ref().top() - nargs {
            return api("the coroutine must be below the arguments");
        }
        self.host_level();
        let st = self.st();
        let mut co = State::from_thread(th);
        if co.same_thread(st) {
            let t = st.top();
            st.set_top(t - nargs);
            return Err(Error::runtime("cannot resume non-suspended coroutine"));
        }
        if !co.check_stack_api(nargs) {
            let t = st.top();
            st.set_top(t - nargs);
            return Err(Error::runtime("too many arguments to resume"));
        }
        st.xmove(&mut co, nargs);
        let (status, nres) = co.resume(Some(&*st), nargs);
        if status == Status::Ok || status == Status::Yield {
            if !st.check_stack_api(nres + 1) {
                let t = co.top();
                co.set_top(t - nres);
                return Err(Error::runtime("too many results to resume"));
            }
            co.xmove(st, nres);
            Ok(if status == Status::Yield {
                Resume::Yield(nres)
            } else {
                Resume::Return(nres)
            })
        } else {
            co.xmove(st, 1);
            Err(catch(st, status, true))
        }
    }

    /// Status of the coroutine at `idx`, as `coroutine.status` reports it.
    pub fn thread_status(&self, idx: i32) -> Result<ThreadStatus> {
        let th = match self.value(idx)?.as_thread() {
            Some(t) => t,
            None => return api("thread expected"),
        };
        let co = State::from_thread(th);
        Ok(callback::aux_status(self.st_ref(), &co))
    }

    /// Close a suspended or dead coroutine at `idx`: run its pending to-be-closed variables and
    /// mark it dead (`lua_closethread`, `coroutine.close`). Returns the error of the coroutine
    /// (or of a `__close` method), if any.
    pub fn close_thread(&mut self, idx: i32) -> Result<()> {
        let th = match self.value(idx)?.as_thread() {
            Some(t) => t,
            None => return api("thread expected"),
        };
        self.room(1)?;
        let st = self.st();
        let mut co = State::from_thread(th);
        match callback::aux_status(st, &co) {
            ThreadStatus::Dead | ThreadStatus::Suspended => {
                let s = co.close_thread(Some(&*st));
                if s == Status::Ok {
                    Ok(())
                } else {
                    co.xmove(st, 1);
                    Err(catch(st, s, false))
                }
            }
            ThreadStatus::Running | ThreadStatus::Normal => {
                api("cannot close a running or normal coroutine")
            }
        }
    }

    /// Can the running function yield (`lua_isyieldable`)?
    pub fn is_yieldable(&self) -> bool {
        self.st_ref().yieldable()
    }

    /// Yield the top `nresults` values from a Rust function: `return s.yield_(n);`
    /// (`lua_yield`). When the coroutine is resumed, the Rust function returns to its caller with
    /// the values passed to `resume` as its results. Returns an error of kind
    /// [`ErrorKind::Yield`] that must be returned unchanged; yielding where it is not allowed (the
    /// main thread, across a [`call`](trait.Stack.html#method.call)) raises a Lua error instead.
    pub fn yield_(&mut self, nresults: usize) -> Result<usize> {
        self.need(nresults)?;
        Err(Error::yield_marker(nresults))
    }

    // ---- garbage collector --------------------------------------------------------------------

    fn gc_usable(&self) -> bool {
        use rn_core::gc::{GCSTP_CLOSING, GCSTP_GC};
        self.st_ref().g().heap.gc_stp() & (GCSTP_GC | GCSTP_CLOSING) == 0
    }

    /// Run a full collection cycle (`collectgarbage("collect")`). Finalizers may run.
    pub fn gc_collect(&mut self) {
        if self.gc_usable() {
            self.st().collect(false);
        }
    }

    /// Perform a basic collection step (`collectgarbage("step")`); returns whether a cycle
    /// finished.
    pub fn gc_step(&mut self) -> bool {
        if !self.gc_usable() {
            return false;
        }
        #[cfg(not(rustynail_lua54))]
        let r = self.st().gc_api_step(0);
        #[cfg(rustynail_lua54)]
        let r = self.st().gc_api_step54(0);
        r
    }

    /// Stop the collector (`collectgarbage("stop")`); allocation still works.
    pub fn gc_stop(&mut self) {
        self.st().heap().gc_stop();
    }

    /// Restart a stopped collector (`collectgarbage("restart")`).
    pub fn gc_restart(&mut self) {
        self.st().heap().gc_restart();
    }

    /// Is the collector running (`collectgarbage("isrunning")`)?
    pub fn gc_is_running(&self) -> bool {
        self.st_ref().g().heap.gc_is_running()
    }

    /// Bytes currently allocated by the state (`collectgarbage("count")` * 1024).
    pub fn gc_count(&self) -> usize {
        self.st_ref().g().heap.total_bytes()
    }

    /// Switch the collector mode; returns the previous one (`collectgarbage("incremental")` /
    /// `collectgarbage("generational")`).
    pub fn set_gc_mode(&mut self, mode: GcMode) -> GcMode {
        use rn_core::gc::GcKind;
        if !self.gc_usable() {
            return mode;
        }
        let new = match mode {
            GcMode::Incremental => GcKind::Incremental,
            GcMode::Generational => GcKind::GenMinor,
        };
        if self.st().gc_change_mode(new).is_generational() {
            GcMode::Generational
        } else {
            GcMode::Incremental
        }
    }

    /// Lua 5.5 only: read and optionally set a collector parameter (`lua_gc(L, LUA_GCPARAM, p,
    /// v)`, `collectgarbage("param", ...)`); returns the previous value. Values are percentages
    /// (pause, step multiplier, the generational multipliers) or bytes (step size).
    #[cfg(not(rustynail_lua54))]
    pub fn gc_param(&mut self, param: crate::GcParam, value: Option<u32>) -> u32 {
        let p = self.st().heap().gc_params();
        let i = param as usize;
        let old = p.get(i);
        if let Some(v) = value {
            p.set(i, v);
        }
        old.clamp(0, i64::from(u32::MAX)) as u32
    }

    /// Lua 5.4 only: set the incremental collector's parameters (`lua_gc(L, LUA_GCINC, pause,
    /// stepmul, stepsize)`; 0 keeps a value) and switch to incremental mode. Returns the previous
    /// mode.
    #[cfg(rustynail_lua54)]
    pub fn gc_incremental(&mut self, pause: i32, stepmul: i32, stepsize: i32) -> GcMode {
        let p = self.st().heap().gc_params();
        if pause != 0 {
            p.set_pause54(i64::from(pause));
        }
        if stepmul != 0 {
            p.set_stepmul54(i64::from(stepmul));
        }
        if stepsize != 0 {
            p.set_stepsize54(i64::from(stepsize));
        }
        self.set_gc_mode(GcMode::Incremental)
    }

    /// Lua 5.4 only: set the generational collector's parameters (`lua_gc(L, LUA_GCGEN,
    /// minormul, majormul)`; 0 keeps a value) and switch to generational mode. Returns the
    /// previous mode.
    #[cfg(rustynail_lua54)]
    pub fn gc_generational(&mut self, minormul: i32, majormul: i32) -> GcMode {
        let p = self.st().heap().gc_params();
        if minormul != 0 {
            p.set_genminormul54(i64::from(minormul));
        }
        if majormul != 0 {
            p.set_genmajormul54(i64::from(majormul));
        }
        self.set_gc_mode(GcMode::Generational)
    }
}
