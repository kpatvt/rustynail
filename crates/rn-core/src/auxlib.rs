//! Rust-side `lauxlib` plus the `lua_*` API pieces the standard libraries need on top of the State
//! layer (metamethod-aware `index`/`set_index`/`len`/comparisons, `lua_getinfo`/`lua_getstack`,
//! `luaL_traceback`, `luaL_ref`, `luaL_loadfilex`, the warning system, argument checking with the
//! reference's exact messages).
//!
//! Conventions:
//! * Argument numbers (`n`) are **1-based**, as in `luaL_check*`; `st.arg(n - 1)` is the value.
//! * Raisers (`arg_error`, `l_error`, ...) return the [`Status`] to put in `Err(..)` after pushing
//!   the error object, exactly like [`State::run_error`].
//! * `Value`s returned from helpers are **not** GC-rooted; push them (or keep them on the stack)
//!   before the next allocation.
#![deny(unsafe_code)]

use crate::debuginfo;
use crate::error::Opnd;
use crate::meta::Tm;
use crate::number::{self, Number, F2I};
use crate::state::*;
use crate::status::{LuaResult, Status};
use crate::string::StrRef;
use crate::table::TableRef;
use crate::value::*;

/// Size of the stack buffer of [`State::where_buf`] (`chunk:line: ` is at most 72 bytes).
pub const WHERE_BUF: usize = 80;

/// `LUA_REFNIL`.
pub const LUA_REFNIL: i32 = -1;
/// `LUA_NOREF`.
pub const LUA_NOREF: i32 = -2;
/// Registry key of the table of loaded modules.
pub const LUA_LOADED_TABLE: &[u8] = b"_LOADED";
/// Registry key of the table of preloaders.
pub const LUA_PRELOAD_TABLE: &[u8] = b"_PRELOAD";
/// Metatable name of file handles.
pub const LUA_FILEHANDLE: &[u8] = b"FILE*";
/// `LUAL_BUFFERSIZE`-independent maximum length for `string.rep` & co. (`MAX_SIZE`).
pub const MAX_SIZE: usize = i64::MAX as usize;

#[cfg(not(rustynail_lua54))]
const FREELIST: i64 = 1;
#[cfg(rustynail_lua54)]
const FREELIST: i64 = LUA_RIDX_LAST + 1;

/// Number of levels shown at the start / end of a long traceback.
const LEVELS1: i32 = 10;
const LEVELS2: i32 = 11;

/// `lua_Debug`, filled by [`State::get_info_ci`].
#[derive(Clone, Debug, Default)]
pub struct DebugInfo {
    pub name: Option<Vec<u8>>,
    pub namewhat: &'static str,
    pub what: &'static str,
    pub source: Vec<u8>,
    pub short_src: Vec<u8>,
    pub currentline: i32,
    pub linedefined: i32,
    pub lastlinedefined: i32,
    pub nups: u8,
    pub nparams: u8,
    pub isvararg: bool,
    pub istailcall: bool,
    pub extraargs: u8,
    pub ftransfer: i32,
    pub ntransfer: i32,
    /// Set when option `L` was requested and the function is a Lua function.
    pub activelines: Option<Vec<i32>>,
    /// The function (always filled).
    pub func: Value,
}

/// Name of the basic type of `v` for `luaL_typename`.
pub fn typename_of(v: &Value) -> &'static [u8] {
    crate::meta::type_name(v.type_tag())
}

impl State {
    // =========================================================================================
    // pushing & string helpers
    // =========================================================================================

    /// Create a string value from bytes, push it and run a GC step (`lua_pushlstring`).
    pub fn push_bytes(&mut self, b: &[u8]) -> LuaResult<Value> {
        let v = self.new_str_value(b)?;
        self.push(v);
        self.check_gc();
        Ok(v)
    }

    /// Push a string of exactly `len` bytes written by `fill` (like `luaL_buffinitsize` +
    /// `luaL_pushresultsize`, but without an intermediate buffer for long strings). `fill` must
    /// initialise every byte.
    pub fn push_filled<F>(&mut self, len: usize, fill: F) -> LuaResult<Value>
    where
        F: FnOnce(&mut [std::mem::MaybeUninit<u8>]),
    {
        let mut fill = Some(fill);
        let s = self.try_alloc(|h| h.new_str_filled(len, &mut fill))?;
        let v = Value::string(s);
        self.push(v);
        self.check_gc();
        Ok(v)
    }

    /// Push the result of a library buffer (`luaL_pushresult`): like [`State::push_bytes`], but a big
    /// result is kept out of the collector's accounting as in the reference.
    pub fn push_buffer(&mut self, b: &[u8]) -> LuaResult<Value> {
        if b.len() <= crate::heap::BUFFER_STR_MIN {
            return self.push_bytes(b);
        }
        self.push_filled(b.len(), |out| {
            for (o, c) in out.iter_mut().zip(b) {
                o.write(*c);
            }
        })
    }

    /// Push a string made of the bytes of `v` (`luaL_pushresult` on a heap buffer): long results
    /// are adopted without a copy.
    pub fn push_vec(&mut self, v: Vec<u8>) -> LuaResult<Value> {
        let s = self.heap().new_str_from_vec(v)?; // (no emergency retry: `v` is consumed)
        let val = Value::string(s);
        self.push(val);
        self.check_gc();
        Ok(val)
    }

    /// B3-2: take the reusable library buffer (empty, with the capacity of earlier uses). Give it back with
    /// [`State::push_scratch`] (result) or [`State::give_scratch`] (no result); if it is lost (error path) the next call
    /// just allocates a new one.
    #[inline]
    pub fn take_scratch(&mut self) -> Vec<u8> {
        let mut v = std::mem::take(&mut self.g_mut().scratch);
        v.clear();
        v
    }

    /// Return a buffer taken by [`State::take_scratch`] (dropped when it is big, to bound the idle memory).
    #[inline]
    pub fn give_scratch(&mut self, v: Vec<u8>) {
        if v.capacity() <= 1 << 16 {
            self.g_mut().scratch = v;
        }
    }

    /// Push the contents of a buffer from [`State::take_scratch`] as a string and keep the buffer for reuse
    /// (a long result in 5.5 adopts the block like [`State::push_vec`] instead).
    pub fn push_scratch(&mut self, v: Vec<u8>) -> LuaResult<Value> {
        if v.len() <= crate::heap::BUFFER_STR_MIN || cfg!(rustynail_lua54) {
            let r = self.push_bytes(&v);
            self.give_scratch(v);
            r
        } else {
            self.push_vec(v)
        }
    }

    /// `lua_pushstring` for a Rust `&str`.
    pub fn push_str(&mut self, s: &str) -> LuaResult<Value> {
        self.push_bytes(s.as_bytes())
    }

    /// Push `v`.
    #[inline]
    pub fn push_value(&mut self, v: Value) {
        self.push(v);
    }

    /// Push `nil` + message (the `luaL_pushfail` + message idiom), returns 2.
    pub fn push_fail_msg(&mut self, msg: &[u8]) -> LuaResult<usize> {
        self.push(Value::NIL);
        self.push_bytes(msg)?;
        Ok(2)
    }

    /// Does the running native receive at least `n` arguments (`lua_gettop >= n`)?
    #[inline]
    pub fn has_arg(&self, n: usize) -> bool {
        self.nargs() >= n
    }

    /// `lua_isnoneornil(L, n)`.
    #[inline]
    pub fn arg_is_none_or_nil(&self, n: usize) -> bool {
        n > self.nargs() || self.arg(n - 1).is_nil()
    }

    /// `luaL_typename(L, n)` (`"no value"` past the top).
    pub fn arg_typename(&self, n: usize) -> &'static [u8] {
        if n > self.nargs() {
            b"no value"
        } else {
            typename_of(&self.arg(n - 1))
        }
    }

    /// Convert a number or numeric string value to a [`Number`] (`cvt2num`).
    #[inline]
    pub fn to_number(v: &Value) -> Option<Number> {
        if let Some(n) = v.as_number() {
            return Some(n);
        }
        v.as_string().and_then(|s| number::str2num(s.as_bytes()))
    }

    /// `lua_tointegerx`: integers, integral floats and numeric strings convertible to such.
    #[inline]
    pub fn to_integer(v: &Value) -> Option<i64> {
        if let Some(i) = v.as_int() {
            return Some(i);
        }
        crate::error::tointeger_ns(v)
    }

    /// `lua_tonumberx`.
    #[inline]
    pub fn to_float(v: &Value) -> Option<f64> {
        match Self::to_number(v)? {
            Number::Int(i) => Some(i as f64),
            Number::Float(f) => Some(f),
        }
    }

    // =========================================================================================
    // errors
    // =========================================================================================

    /// `luaL_where(level)`: `"chunk:line: "` of the function at `level` (empty if unknown).
    pub fn where_(&mut self, level: usize) -> Vec<u8> {
        let mut buf = [0u8; WHERE_BUF];
        let n = self.where_buf(level, &mut buf);
        buf[..n].to_vec()
    }

    /// [`State::where_`] without allocation (the `error()` path of `pcall`-heavy code): writes `"chunk:line: "` into `buf`, returns its length (0 if unknown).
    /// Byte-identical to the `get_info_ci(ci, "Sl")` formulation it replaces (only Lua functions have a current line).
    #[inline(never)]
    pub fn where_buf(&self, level: usize, buf: &mut [u8; WHERE_BUF]) -> usize {
        let Some(ci) = self.get_stack(level) else { return 0 };
        if !ci.r().is_lua() {
            return 0;
        }
        let Some(cl) = self.ci_lclosure(ci) else { return 0 };
        let p = cl.proto();
        let line = p.line_at(self.current_pc(ci));
        if line <= 0 {
            return 0;
        }
        let mut id = [0u8; debuginfo::LUA_IDSIZE];
        let n = match p.source() {
            Some(s) => debuginfo::chunk_id_buf(s.get().as_bytes(), &mut id),
            None => debuginfo::chunk_id_buf(b"=?", &mut id),
        };
        buf[..n].copy_from_slice(&id[..n]);
        let mut n = n;
        buf[n] = b':';
        n += 1;
        // decimal digits of a positive i32 (at most 10)
        let mut tmp = [0u8; 10];
        let mut i = tmp.len();
        let mut v = line as u32;
        while v != 0 {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        let d = tmp.len() - i;
        buf[n..n + d].copy_from_slice(&tmp[i..]);
        n += d;
        buf[n] = b':';
        buf[n + 1] = b' ';
        n + 2
    }

    /// Creates the string `where(level) .. msg` (the message of `luaL_error` / `error(msg, level)`), without intermediate vectors for messages that fit a stack buffer.
    #[inline(never)]
    pub fn new_where_str(&mut self, level: usize, msg: &[u8]) -> LuaResult<Value> {
        let mut buf = [0u8; WHERE_BUF + 192];
        let n = self.where_buf(level, (&mut buf[..WHERE_BUF]).try_into().unwrap());
        if n + msg.len() <= buf.len() {
            buf[n..n + msg.len()].copy_from_slice(msg);
            self.new_str_value(&buf[..n + msg.len()])
        } else {
            let mut full = Vec::with_capacity(n + msg.len());
            full.extend_from_slice(&buf[..n]);
            full.extend_from_slice(msg);
            self.new_str_value(&full)
        }
    }

    /// Raise a non-string error value (`lua_error`).
    pub fn raise(&mut self, v: Value) -> Status {
        self.throw_value(v)
    }

    /// `luaL_error`: message prefixed with `luaL_where(1)`.
    #[inline(never)]
    pub fn l_error(&mut self, msg: &[u8]) -> Status {
        match self.new_where_str(1, msg) {
            Ok(v) => self.throw_value(v),
            Err(e) => e,
        }
    }

    /// `luaL_error` with a `&str` message.
    pub fn l_error_str(&mut self, msg: &str) -> Status {
        self.l_error(msg.as_bytes())
    }

    /// `luaL_argerror`.
    pub fn arg_error(&mut self, arg: usize, extramsg: &[u8]) -> Status {
        let mut arg = arg as i64;
        let Some(ci) = self.get_stack(0) else {
            return self.l_error(format!("bad argument #{arg} ({})", lossy(extramsg)).as_bytes());
        };
        let info = self.get_info_ci(ci, b"nt").unwrap_or_default();
        #[allow(unused_mut)]
        let mut argword = "argument";
        #[cfg(not(rustynail_lua54))]
        {
            if arg <= info.extraargs as i64 {
                argword = "extra argument";
            } else {
                arg -= info.extraargs as i64;
            }
        }
        if argword == "argument" && info.namewhat == "method" {
            arg -= 1;
            if arg == 0 {
                let name = info.name.clone().unwrap_or_default();
                let mut m = b"calling '".to_vec();
                m.extend_from_slice(&name);
                m.extend_from_slice(b"' on bad self (");
                m.extend_from_slice(extramsg);
                m.push(b')');
                return self.l_error(&m);
            }
        }
        let name = match info.name.clone() {
            Some(n) => n,
            None => {
                let f = info.func;
                self.push_global_func_name(&f)
                    .unwrap_or_else(|| b"?".to_vec())
            }
        };
        let mut m = format!("bad {argword} #{arg} to '").into_bytes();
        m.extend_from_slice(&name);
        m.extend_from_slice(b"' (");
        m.extend_from_slice(extramsg);
        m.push(b')');
        self.l_error(&m)
    }

    /// `luaL_typeerror`.
    pub fn type_error_arg(&mut self, arg: usize, tname: &[u8]) -> Status {
        let v = if arg <= self.nargs() {
            Some(self.arg(arg - 1))
        } else {
            None
        };
        let typearg: Vec<u8> = match v {
            Some(v) => {
                let mt_name = if v.is_table() || v.is_userdata() {
                    self.get_metafield(&v, b"__name")
                } else {
                    Value::NIL
                };
                if let Some(s) = mt_name.as_string() {
                    s.as_bytes().to_vec()
                } else if v.is_light_userdata() {
                    b"light userdata".to_vec()
                } else {
                    typename_of(&v).to_vec()
                }
            }
            None => b"no value".to_vec(),
        };
        let mut m = tname.to_vec();
        m.extend_from_slice(b" expected, got ");
        m.extend_from_slice(&typearg);
        self.arg_error(arg, &m)
    }

    /// `luaL_argexpected`/`luaL_argcheck`: error unless `cond`.
    #[inline]
    pub fn arg_check(&mut self, cond: bool, arg: usize, msg: &[u8]) -> LuaResult<()> {
        if cond {
            Ok(())
        } else {
            Err(self.arg_error(arg, msg))
        }
    }

    // =========================================================================================
    // argument checking
    // =========================================================================================

    /// `luaL_checkany`.
    #[inline]
    pub fn check_any(&mut self, n: usize) -> LuaResult<Value> {
        if n > self.nargs() {
            return Err(self.arg_error(n, b"value expected"));
        }
        Ok(self.arg(n - 1))
    }

    /// `luaL_checktype` for the basic types.
    pub fn check_type(&mut self, n: usize, t: LuaType) -> LuaResult<Value> {
        let v = self.arg(n - 1);
        if n > self.nargs() || v.type_of() != t {
            let name: &[u8] = match t {
                LuaType::Nil => b"nil",
                LuaType::Boolean => b"boolean",
                LuaType::LightUserdata => b"userdata",
                LuaType::Number => b"number",
                LuaType::String => b"string",
                LuaType::Table => b"table",
                LuaType::Function => b"function",
                LuaType::Userdata => b"userdata",
                LuaType::Thread => b"thread",
            };
            return Err(self.type_error_arg(n, name));
        }
        Ok(v)
    }

    /// Argument `n` must be a table.
    #[inline]
    pub fn check_table(&mut self, n: usize) -> LuaResult<TableRef> {
        match self.arg(n - 1).as_table() {
            Some(t) if n <= self.nargs() => Ok(t),
            _ => Err(self.type_error_arg(n, b"table")),
        }
    }

    /// `luaL_checkinteger`.
    #[inline]
    pub fn check_integer(&mut self, n: usize) -> LuaResult<i64> {
        let v = self.arg(n - 1);
        if let Some(i) = v.as_int() {
            return Ok(i);
        }
        self.check_integer_slow(n, v)
    }

    #[cold]
    fn check_integer_slow(&mut self, n: usize, v: Value) -> LuaResult<i64> {
        match Self::to_integer(&v) {
            Some(i) => Ok(i),
            None => {
                if Self::to_number(&v).is_some() {
                    Err(self.arg_error(n, b"number has no integer representation"))
                } else {
                    Err(self.type_error_arg(n, b"number"))
                }
            }
        }
    }

    /// `luaL_optinteger`.
    #[inline]
    pub fn opt_integer(&mut self, n: usize, def: i64) -> LuaResult<i64> {
        if self.arg_is_none_or_nil(n) {
            Ok(def)
        } else {
            self.check_integer(n)
        }
    }

    /// `luaL_checknumber`.
    #[inline]
    pub fn check_number(&mut self, n: usize) -> LuaResult<f64> {
        let v = self.arg(n - 1);
        if let Some(f) = v.as_float() {
            return Ok(f);
        }
        if let Some(i) = v.as_int() {
            return Ok(i as f64);
        }
        match Self::to_float(&v) {
            Some(f) => Ok(f),
            None => Err(self.type_error_arg(n, b"number")),
        }
    }

    /// Number argument keeping the int/float distinction (after string coercion).
    pub fn check_num(&mut self, n: usize) -> LuaResult<Number> {
        let v = self.arg(n - 1);
        match Self::to_number(&v) {
            Some(x) => Ok(x),
            None => Err(self.type_error_arg(n, b"number")),
        }
    }

    /// `luaL_optnumber`.
    #[inline]
    pub fn opt_number(&mut self, n: usize, def: f64) -> LuaResult<f64> {
        if self.arg_is_none_or_nil(n) {
            Ok(def)
        } else {
            self.check_number(n)
        }
    }

    /// `luaL_checklstring`: strings and numbers (numbers are converted **in place**, as `lua_tolstring`).
    #[inline]
    pub fn check_str(&mut self, n: usize) -> LuaResult<StrRef> {
        let v = self.arg(n - 1);
        if n <= self.nargs() {
            if let Some(s) = v.as_strref() {
                return Ok(s);
            }
        }
        self.check_str_slow(n, v)
    }

    #[cold]
    fn check_str_slow(&mut self, n: usize, v: Value) -> LuaResult<StrRef> {
        if n <= self.nargs() {
            if let Some(num) = v.as_number() {
                let buf = number::tostring(num);
                let s = self.new_str(buf.as_bytes())?;
                let base = self.ci().r().func;
                self.set(base + n, Value::string(s));
                return Ok(s);
            }
        }
        Err(self.type_error_arg(n, b"string"))
    }

    /// `luaL_optlstring`.
    #[inline]
    pub fn opt_str(&mut self, n: usize) -> LuaResult<Option<StrRef>> {
        if self.arg_is_none_or_nil(n) {
            Ok(None)
        } else {
            self.check_str(n).map(Some)
        }
    }

    /// `luaL_checkoption`: index of the argument in `lst`.
    pub fn check_option(&mut self, n: usize, def: Option<&str>, lst: &[&str]) -> LuaResult<usize> {
        let name: Vec<u8> = match (def, self.arg_is_none_or_nil(n)) {
            (Some(d), true) => d.as_bytes().to_vec(),
            _ => self.check_str(n)?.get().as_bytes().to_vec(),
        };
        if let Some(i) = lst.iter().position(|o| o.as_bytes() == name.as_slice()) {
            return Ok(i);
        }
        let mut m = b"invalid option '".to_vec();
        m.extend_from_slice(&name);
        m.push(b'\'');
        Err(self.arg_error(n, &m))
    }

    /// `luaL_checkstack`.
    pub fn check_stack_msg(&mut self, space: usize, msg: &str) -> LuaResult<()> {
        if self.check_stack(space).is_err() {
            // the error text of the failed attempt is discarded (it is "stack overflow" anyway)
            return Err(self.l_error(format!("stack overflow ({msg})").as_bytes()));
        }
        Ok(())
    }

    // =========================================================================================
    // metatables / metafields
    // =========================================================================================

    /// `luaL_getmetafield`: the raw value of `event` in `v`'s metatable (nil if none).
    pub fn get_metafield(&mut self, v: &Value, event: &[u8]) -> Value {
        match self.metatable_of(v) {
            None => Value::NIL,
            Some(mt) => match self.new_str(event) {
                Ok(k) => mt.get_str(k),
                Err(_) => Value::NIL,
            },
        }
    }

    /// `luaL_getmetafield(v, "__tostring")` without interning the key on every call (nil if none).
    #[inline]
    pub fn tostring_metafield(&self, v: &Value) -> Value {
        match self.metatable_of(v) {
            None => Value::NIL,
            Some(mt) => mt.get_str(self.g().tostring_key),
        }
    }

    /// `luaL_callmeta`: call `v`'s `event` metamethod with `v`; returns the (unrooted) result.
    pub fn call_meta(&mut self, v: &Value, event: &[u8]) -> LuaResult<Option<Value>> {
        let f = self.get_metafield(v, event);
        if f.is_nil() {
            return Ok(None);
        }
        let r = self.call_value1(f, &[*v])?;
        Ok(Some(r))
    }

    // =========================================================================================
    // calls
    // =========================================================================================

    /// Call `f(args...)` (non-yieldable, `lua_call` from a native without continuation) and leave
    /// `nres` results (or all of them for `LUA_MULTRET`) on the stack.
    pub fn call_value(&mut self, f: Value, args: &[Value], nres: i32) -> LuaResult<()> {
        self.check_stack(args.len() + 2)?;
        let func = self.top();
        self.push(f);
        for a in args {
            self.push(*a);
        }
        self.call_no_yield(func, nres)
    }

    /// Call and return the single (unrooted) result, popping it.
    pub fn call_value1(&mut self, f: Value, args: &[Value]) -> LuaResult<Value> {
        self.call_value(f, args, 1)?;
        Ok(self.pop())
    }

    /// Protected call of `f(args...)` with `nres` results; on error the error object is returned in
    /// `Err((status, value))` and the stack is restored.
    pub fn pcall_value(
        &mut self,
        f: Value,
        args: &[Value],
        nres: i32,
    ) -> Result<(), (Status, Value)> {
        if self.check_stack(args.len() + 2).is_err() {
            return Err((Status::ErrRun, Value::NIL));
        }
        let func = self.top();
        self.push(f);
        for a in args {
            self.push(*a);
        }
        let st = self.pcall(func, nres, 0);
        if st != Status::Ok {
            let e = self.pop();
            return Err((st, e));
        }
        Ok(())
    }

    // =========================================================================================
    // metamethod-aware operations
    // =========================================================================================

    /// `lua_gettable` semantics (`luaV_finishget`): `obj[key]` with `__index`.
    pub fn index(&mut self, obj: Value, key: Value) -> LuaResult<Value> {
        let mut obj = obj;
        for _ in 0..2000 {
            let tm;
            if let Some(t) = obj.as_table() {
                let v = t.get(&key);
                if !v.is_nil() {
                    return Ok(v);
                }
                match self.fast_tm(t.metatable(), Tm::Index) {
                    None => return Ok(Value::NIL),
                    Some(m) => tm = m,
                }
            } else {
                tm = self.get_tm_by_obj(&obj, Tm::Index);
                if tm.is_nil() {
                    return Err(self.type_error(Opnd::from(obj), "index"));
                }
            }
            if tm.is_function() {
                let res = self.top();
                return self.call_tm_res(tm, obj, key, res);
            }
            obj = tm;
        }
        Err(self.run_error(b"'__index' chain too long; possible loop"))
    }

    /// `obj[i]` with metamethods (`lua_geti`).
    #[inline]
    pub fn index_int(&mut self, obj: Value, i: i64) -> LuaResult<Value> {
        if let Some(t) = obj.as_table() {
            let v = t.get_int(i);
            if !v.is_nil() || t.metatable().is_none() {
                return Ok(v);
            }
        }
        self.index(obj, Value::int(i))
    }

    /// `obj[name]` with metamethods (`lua_getfield`).
    pub fn index_str(&mut self, obj: Value, name: &[u8]) -> LuaResult<Value> {
        let k = self.new_str_value(name)?;
        self.push(k);
        let r = self.index(obj, k);
        if r.is_ok() {
            self.pop(); // on error the error object is on top: leave it
        }
        r
    }

    /// `lua_settable` semantics (`luaV_finishset`): `obj[key] = val` with `__newindex`.
    pub fn set_index(&mut self, obj: Value, key: Value, val: Value) -> LuaResult<()> {
        let mut obj = obj;
        for _ in 0..2000 {
            let tm;
            if let Some(t) = obj.as_table() {
                let cur = t.get(&key);
                let m = if cur.is_nil() {
                    self.fast_tm(t.metatable(), Tm::NewIndex)
                } else {
                    None
                };
                match m {
                    None => return self.raw_set(t, &key, &val),
                    Some(m) => tm = m,
                }
            } else {
                tm = self.get_tm_by_obj(&obj, Tm::NewIndex);
                if tm.is_nil() {
                    return Err(self.type_error(Opnd::from(obj), "index"));
                }
            }
            if tm.is_function() {
                return self.call_tm(tm, obj, key, val);
            }
            obj = tm;
        }
        Err(self.run_error(b"'__newindex' chain too long; possible loop"))
    }

    /// `obj[i] = val` with metamethods (`lua_seti`).
    #[inline]
    pub fn set_index_int(&mut self, obj: Value, i: i64, val: Value) -> LuaResult<()> {
        if let Some(t) = obj.as_table() {
            if t.metatable().is_none() {
                return self.raw_set_int(t, i, &val);
            }
        }
        self.set_index(obj, Value::int(i), val)
    }

    /// `obj[name] = val` with metamethods (`lua_setfield`).
    pub fn set_index_str(&mut self, obj: Value, name: &[u8], val: Value) -> LuaResult<()> {
        let k = self.new_str_value(name)?;
        self.push(k);
        let r = self.set_index(obj, k, val);
        if r.is_ok() {
            self.pop(); // on error the error object is on top: leave it
        }
        r
    }

    /// `lua_rawlen`.
    pub fn raw_len(&self, v: &Value) -> u64 {
        if let Some(s) = v.as_string() {
            s.len() as u64
        } else if let Some(t) = v.as_table() {
            t.len(&self.g().heap)
        } else if let Some(u) = v.as_userdata() {
            u.len() as u64
        } else {
            0
        }
    }

    /// `lua_len` (`luaV_objlen`): the `#` operator with `__len`.
    pub fn len_value(&mut self, v: Value) -> LuaResult<Value> {
        let tm;
        if let Some(t) = v.as_table() {
            match self.fast_tm(t.metatable(), Tm::Len) {
                None => return Ok(Value::int(t.len(&self.g().heap) as i64)),
                Some(m) => tm = m,
            }
        } else if let Some(s) = v.as_string() {
            return Ok(Value::int(s.len() as i64));
        } else {
            tm = self.get_tm_by_obj(&v, Tm::Len);
            if tm.is_nil() {
                return Err(self.type_error(Opnd::from(v), "get length of"));
            }
        }
        let res = self.top();
        self.call_tm_res(tm, v, v, res)
    }

    /// `luaL_len`.
    pub fn len_int(&mut self, v: Value) -> LuaResult<i64> {
        let r = self.len_value(v)?;
        match Self::to_integer(&r) {
            Some(i) => Ok(i),
            None => Err(self.l_error(b"object length is not an integer")),
        }
    }

    /// `lua_compare(.., LUA_OPEQ)`.
    pub fn equals(&mut self, a: Value, b: Value) -> LuaResult<bool> {
        if a.raw_equal(&b) {
            return Ok(true);
        }
        let tm = if a.is_table() && b.is_table() {
            let m = self.fast_tm(a.as_table().and_then(|t| t.metatable()), Tm::Eq);
            m.or_else(|| self.fast_tm(b.as_table().and_then(|t| t.metatable()), Tm::Eq))
        } else if a.is_userdata() && b.is_userdata() {
            let t = self.get_tm_by_obj(&a, Tm::Eq);
            let t = if t.is_nil() {
                self.get_tm_by_obj(&b, Tm::Eq)
            } else {
                t
            };
            if t.is_nil() {
                None
            } else {
                Some(t)
            }
        } else {
            None
        };
        match tm {
            None => Ok(false),
            Some(tm) => {
                let res = self.top();
                Ok(self.call_tm_res(tm, a, b, res)?.is_truthy())
            }
        }
    }

    /// `lua_compare(.., LUA_OPLT)` / `LUA_OPLE` (`le`).
    pub fn compare_lt_le(&mut self, a: Value, b: Value, le: bool) -> LuaResult<bool> {
        if let (Some(x), Some(y)) = (a.as_number(), b.as_number()) {
            return Ok(if le {
                number::le_num(x, y)
            } else {
                number::lt_num(x, y)
            });
        }
        if let (Some(x), Some(y)) = (a.as_string(), b.as_string()) {
            return Ok(if le {
                x.as_bytes() <= y.as_bytes()
            } else {
                x.as_bytes() < y.as_bytes()
            });
        }
        self.call_order_tm(a, b, if le { Tm::Le } else { Tm::Lt })
    }

    // =========================================================================================
    // luaL_tolstring
    // =========================================================================================

    /// Printable address of a value for `%p`.
    pub fn value_pointer(&self, v: &Value) -> usize {
        if let Some(p) = v.as_light_userdata() {
            p as usize
        } else if let Some(f) = v.as_native() {
            f as usize
        } else if let Some(f) = v.as_light_cfn() {
            f as usize
        } else if let Some(g) = v.as_gc() {
            g as usize
        } else {
            0
        }
    }

    /// `luaL_tolstring`: converts any value to a string, honouring `__tostring` / `__name`. The
    /// result is **left on the stack** (callers pop it when done).
    pub fn tolstring(&mut self, v: Value) -> LuaResult<StrRef> {
        // a value without a metatable has neither `__tostring` nor `__name`: skip both (string-keyed) lookups
        let mt = self.metatable_of(&v);
        if let Some(mt) = mt {
            let f = mt.get_str(self.g().tostring_key);
            let called = if f.is_nil() { None } else { Some(self.call_value1(f, &[v])?) };
            if let Some(r) = called {
                if let Some(s) = r.as_strref() {
                    self.push(r);
                    return Ok(s);
                }
                if let Some(n) = r.as_number() {
                    let b = number::tostring(n);
                    let s = self.new_str(b.as_bytes())?;
                    self.push(Value::string(s));
                    return Ok(s);
                }
                return Err(self.l_error(b"'__tostring' must return a string"));
            }
        }
        if let Some(s) = v.as_strref() {
            self.push(v);
            return Ok(s);
        }
        let s = if let Some(n) = v.as_number() {
            number::with_tostring(n, |b| self.new_str_fresh(b))?
        } else if let Some(b) = v.as_bool() {
            self.new_str(if b { b"true" } else { b"false" })?
        } else if v.is_nil() {
            self.new_str(b"nil")?
        } else {
            let mt_name = match mt {
                Some(mt) => mt.get_str(self.g().name_key),
                None => Value::NIL,
            };
            // "<kind>: 0x<hex>" built on the stack when the kind is short (the usual case)
            let mut stack = [0u8; 96];
            let mut heap_buf: Vec<u8>;
            let kind: &[u8] = match mt_name.as_string() {
                Some(s) => s.as_bytes(),
                None => typename_of(&v),
            };
            let p = self.value_pointer(&v);
            let out: &mut [u8];
            let mut n = kind.len();
            if kind.len() + 20 <= stack.len() {
                stack[..n].copy_from_slice(kind);
                out = &mut stack[..];
            } else {
                heap_buf = kind.to_vec();
                heap_buf.resize(kind.len() + 20, 0);
                out = &mut heap_buf[..];
            }
            if p == 0 {
                out[n..n + 7].copy_from_slice(b": (nil)");
                n += 7;
            } else {
                out[n] = b':';
                out[n + 1] = b' ';
                out[n + 2] = b'0';
                out[n + 3] = b'x';
                n += 4;
                let nd = (16 - (p.leading_zeros() as usize / 4)).max(1);
                for k in 0..nd {
                    let nib = (p >> (4 * (nd - 1 - k))) & 15;
                    out[n + k] = b"0123456789abcdef"[nib];
                }
                n += nd;
            }
            self.new_str(&out[..n])?
        };
        self.push(Value::string(s));
        self.check_gc();
        Ok(s)
    }

    // =========================================================================================
    // tables / registry / references
    // =========================================================================================

    /// `lua_createtable(narr, nrec)`; the table is **not** pushed.
    pub fn new_table_sized(&mut self, narr: usize, nrec: usize) -> LuaResult<TableRef> {
        let t = self.new_table()?;
        // anchor while resizing and collecting (`lua_createtable`: resize, then `luaC_checkGC`)
        self.push(Value::table(t));
        if narr > 0 || nrec > 0 {
            let r = t.resize(
                self.heap(),
                narr.min(u32::MAX as usize) as u32,
                nrec.min(u32::MAX as usize) as u32,
            );
            if let Err(e) = r {
                self.pop();
                return Err(self.table_error(e));
            }
        }
        self.check_gc();
        self.pop();
        Ok(t)
    }

    /// Registry lookup by string key.
    pub fn registry_get(&mut self, key: &[u8]) -> LuaResult<Value> {
        let k = self.new_str(key)?;
        Ok(self.registry().get_str(k))
    }

    /// Registry store by string key.
    pub fn registry_set(&mut self, key: &[u8], val: Value) -> LuaResult<()> {
        let k = self.new_str_value(key)?;
        self.push(k);
        let r = self.raw_set(self.registry(), &k, &val);
        self.pop();
        r
    }

    /// Raw string-keyed store.
    pub fn raw_set_str(&mut self, t: TableRef, key: &[u8], val: Value) -> LuaResult<()> {
        let k = self.new_str_value(key)?;
        self.push(k);
        let r = self.raw_set(t, &k, &val);
        self.pop();
        r
    }

    /// Raw string-keyed lookup.
    pub fn raw_get_str(&mut self, t: TableRef, key: &[u8]) -> LuaResult<Value> {
        let k = self.new_str(key)?;
        Ok(t.get_str(k))
    }

    /// `luaL_setfuncs` without upvalues: store light natives into `t`.
    pub fn set_funcs(&mut self, t: TableRef, funcs: &[(&str, Option<NativeFn>)]) -> LuaResult<()> {
        self.push(Value::table(t));
        for (name, f) in funcs {
            let v = match f {
                Some(f) => Value::native(*f),
                None => Value::FALSE, // placeholder
            };
            self.raw_set_str(t, name.as_bytes(), v)?;
        }
        self.pop();
        Ok(())
    }

    /// `luaL_getsubtable`: `t[fname]` if a table, else a new table stored there.
    pub fn get_sub_table(&mut self, t: TableRef, fname: &[u8]) -> LuaResult<(TableRef, bool)> {
        let v = self.raw_get_str(t, fname)?;
        if let Some(sub) = v.as_table() {
            return Ok((sub, true));
        }
        let sub = self.new_table()?;
        self.push(Value::table(sub));
        let r = self.raw_set_str(t, fname, Value::table(sub));
        self.pop();
        r?;
        Ok((sub, false))
    }

    /// `luaL_requiref`: open module `modname` with `openf` unless loaded; the module is pushed.
    pub fn requiref(&mut self, modname: &[u8], openf: NativeFn, glb: bool) -> LuaResult<Value> {
        let reg = self.registry();
        let (loaded, _) = self.get_sub_table(reg, LUA_LOADED_TABLE)?;
        self.push(Value::table(loaded));
        let mut m = self.raw_get_str(loaded, modname)?;
        if m.is_falsy() {
            let name = self.new_str_value(modname)?;
            self.call_value(Value::native(openf), &[name], 1)?;
            m = self.get(self.top() - 1);
            self.raw_set_str(loaded, modname, m)?;
        } else {
            self.push(m);
        }
        // stack: ..., loaded, m
        if glb {
            let g = self.globals();
            self.raw_set_str(g, modname, m)?;
        }
        let top = self.top();
        self.set(top - 2, m);
        self.set_top(top - 1);
        Ok(m)
    }

    /// `luaL_ref`: store `v` in table `t` and return its reference.
    pub fn ref_(&mut self, t: TableRef, v: Value) -> LuaResult<i32> {
        if v.is_nil() {
            return Ok(LUA_REFNIL);
        }
        let first = t.get_int(FREELIST);
        let mut r: i64 = match first.as_int() {
            Some(i) => i,
            None => {
                self.raw_set_int(t, FREELIST, &Value::int(0))?;
                0
            }
        };
        if r != 0 {
            let nxt = t.get_int(r);
            self.raw_set_int(t, FREELIST, &nxt)?;
        } else {
            // (5.4: `freelist` is slot 3, so a new reference can be 1, 2 and then 4: `rawlen`
            // already counts the free-list slot)
            r = t.len(&self.g().heap) as i64 + 1;
        }
        self.raw_set_int(t, r, &v)?;
        Ok(r as i32)
    }

    /// `luaL_unref`.
    pub fn unref(&mut self, t: TableRef, r: i32) -> LuaResult<()> {
        if r >= 0 {
            let first = t.get_int(FREELIST);
            self.raw_set_int(t, r as i64, &first)?;
            self.raw_set_int(t, FREELIST, &Value::int(r as i64))?;
        }
        Ok(())
    }

    // =========================================================================================
    // stack frames: lua_getstack / lua_getinfo
    // =========================================================================================

    /// `lua_getstack`: the frame `level` levels up (0 = the running function).
    pub fn get_stack(&self, mut level: usize) -> Option<Ci> {
        let base = self.base_ci();
        let mut ci = self.ci();
        while level > 0 && ci != base {
            level -= 1;
            ci = ci.previous()?;
        }
        if level == 0 && ci != base {
            Some(ci)
        } else {
            None
        }
    }

    /// `funcnamefromcall`.
    fn func_name_from_call(&self, ci: Ci) -> Option<(&'static str, Vec<u8>)> {
        let cs = ci.r().callstatus;
        if cs & CIST_HOOKED != 0 {
            let o = debuginfo::funcname_hook();
            return Some((o.kind, o.name.to_vec()));
        }
        if cs & CIST_FIN != 0 {
            let o = debuginfo::funcname_finalizer();
            return Some((o.kind, o.name.to_vec()));
        }
        if ci.r().is_lua() {
            let cl = self.ci_lclosure(ci)?;
            let p = cl.proto();
            let pc = self.current_pc(ci);
            if pc < 0 {
                return None;
            }
            let o = debuginfo::funcnamefromcode(&*p, pc as usize)?;
            return Some((o.kind, o.name.to_vec()));
        }
        None
    }

    /// `getfuncname`.
    fn get_func_name(&self, ci: Option<Ci>) -> Option<(&'static str, Vec<u8>)> {
        let ci = ci?;
        if ci.r().callstatus & CIST_TAIL != 0 {
            return None;
        }
        let prev = ci.previous()?;
        self.func_name_from_call(prev)
    }

    /// `lua_getinfo` for a running frame (options `S l u t n r L`; `f` is always provided in
    /// `.func`). `None` for an invalid option.
    pub fn get_info_ci(&self, ci: Ci, what: &[u8]) -> Option<DebugInfo> {
        let func = self.get(ci.r().func);
        self.get_info_aux(Some(ci), func, what)
    }

    /// `lua_getinfo(">...")`: information about a function value.
    pub fn get_info_func(&self, func: Value, what: &[u8]) -> Option<DebugInfo> {
        self.get_info_aux(None, func, what)
    }

    fn get_info_aux(&self, ci: Option<Ci>, func: Value, what: &[u8]) -> Option<DebugInfo> {
        let mut ar = DebugInfo {
            func,
            currentline: -1,
            ..Default::default()
        };
        let lcl = func.as_lclosure();
        let ccl = func.as_cclosure();
        let mut ok = true;
        for &c in what {
            match c {
                b'S' => {
                    let si = match lcl {
                        Some(cl) => debuginfo::lua_funcinfo(&*cl.proto()),
                        None => debuginfo::c_funcinfo(),
                    };
                    ar.source = si.source;
                    ar.short_src = si.short_src;
                    ar.linedefined = si.line_defined;
                    ar.lastlinedefined = si.last_line_defined;
                    ar.what = si.what;
                }
                b'l' => {
                    ar.currentline = match ci {
                        Some(ci) if ci.r().is_lua() => self.current_line(ci),
                        _ => -1,
                    };
                }
                b'u' => {
                    if let Some(cl) = lcl {
                        ar.nups = cl.nupvalues() as u8;
                        let p = cl.proto();
                        ar.isvararg = p.is_vararg();
                        ar.nparams = p.numparams;
                    } else {
                        ar.nups = ccl.map_or(0, |c| c.nupvalues() as u8);
                        ar.isvararg = true;
                        ar.nparams = 0;
                    }
                }
                b't' => {
                    if let Some(ci) = ci {
                        ar.istailcall = ci.r().callstatus & CIST_TAIL != 0;
                        ar.extraargs = ((ci.r().callstatus & MAX_CCMT) >> CIST_CCMT) as u8;
                    }
                }
                b'n' => match self.get_func_name(ci) {
                    Some((kind, name)) => {
                        ar.namewhat = kind;
                        ar.name = Some(name);
                    }
                    None => {
                        ar.namewhat = "";
                        ar.name = None;
                    }
                },
                b'r' => match ci {
                    Some(c) if c.r().callstatus & CIST_HOOKED != 0 => {
                        ar.ftransfer = self.t().ftransfer;
                        ar.ntransfer = self.t().ntransfer;
                    }
                    _ => {
                        ar.ftransfer = 0;
                        ar.ntransfer = 0;
                    }
                },
                b'f' => {}
                b'L' => {
                    // stripped Lua functions (no line info) get an empty table, C functions get nil
                    ar.activelines = lcl
                        .map(|cl| debuginfo::collect_valid_lines(&*cl.proto()).unwrap_or_default());
                }
                _ => ok = false,
            }
        }
        if ok {
            Some(ar)
        } else {
            None
        }
    }

    /// `pushglobalfuncname`: `"string.rep"`-style name of a function found in `package.loaded`.
    pub fn push_global_func_name(&mut self, f: &Value) -> Option<Vec<u8>> {
        let loaded = self.registry_get(LUA_LOADED_TABLE).ok()?.as_table()?;
        let name = self.find_field(f, loaded, 2)?;
        if let Some(rest) = name.strip_prefix(b"_G.") {
            Some(rest.to_vec())
        } else {
            Some(name)
        }
    }

    fn find_field(&mut self, obj: &Value, t: TableRef, level: i32) -> Option<Vec<u8>> {
        if level == 0 {
            return None;
        }
        let mut key = Value::NIL;
        loop {
            let (k, v) = t.next(&key).ok()??;
            key = k;
            if let Some(ks) = k.as_string() {
                if obj.raw_equal(&v) {
                    return Some(ks.as_bytes().to_vec());
                }
                if let Some(sub) = v.as_table() {
                    if let Some(rest) = self.find_field(obj, sub, level - 1) {
                        let mut name = ks.as_bytes().to_vec();
                        name.push(b'.');
                        name.extend_from_slice(&rest);
                        return Some(name);
                    }
                }
            }
        }
    }

    fn push_func_name_text(&mut self, ar: &DebugInfo) -> Vec<u8> {
        #[cfg(rustynail_lua54)]
        if let Some(n) = self.push_global_func_name(&ar.func) {
            let mut o = b"function '".to_vec();
            o.extend_from_slice(&n);
            o.push(b'\'');
            return o;
        }
        if !ar.namewhat.is_empty() {
            let mut o = ar.namewhat.as_bytes().to_vec();
            o.extend_from_slice(b" '");
            o.extend_from_slice(ar.name.as_deref().unwrap_or(b"?"));
            o.push(b'\'');
            return o;
        }
        if ar.what == "main" {
            return b"main chunk".to_vec();
        }
        #[cfg(not(rustynail_lua54))]
        if let Some(n) = self.push_global_func_name(&ar.func) {
            let mut o = b"function '".to_vec();
            o.extend_from_slice(&n);
            o.push(b'\'');
            return o;
        }
        if ar.what != "C" {
            let mut o = b"function <".to_vec();
            o.extend_from_slice(&ar.short_src);
            o.push(b':');
            o.extend_from_slice(ar.linedefined.to_string().as_bytes());
            o.push(b'>');
            return o;
        }
        b"?".to_vec()
    }

    fn last_level(&self) -> i32 {
        let mut li = 1;
        let mut le = 1;
        while self.get_stack(le as usize).is_some() {
            li = le;
            le *= 2;
        }
        while li < le {
            let m = (li + le) / 2;
            if self.get_stack(m as usize).is_some() {
                li = m + 1;
            } else {
                le = m;
            }
        }
        le - 1
    }

    /// `luaL_traceback` for the frames of `self` (the thread to inspect).
    pub fn traceback(&mut self, msg: Option<&[u8]>, level: i32) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut level = level;
        let last = self.last_level();
        let mut limit2show: i32 = if last - level > LEVELS1 + LEVELS2 {
            LEVELS1
        } else {
            -1
        };
        if let Some(m) = msg {
            out.extend_from_slice(m);
            out.push(b'\n');
        }
        out.extend_from_slice(b"stack traceback:");
        loop {
            let cur = level;
            level += 1;
            let Some(ci) = (if cur >= 0 {
                self.get_stack(cur as usize)
            } else {
                None
            }) else {
                break;
            };
            let l2 = limit2show;
            limit2show -= 1;
            if l2 == 0 {
                let n = last - level - LEVELS2 + 1;
                out.extend_from_slice(format!("\n\t...\t(skipping {n} levels)").as_bytes());
                level += n;
            } else {
                let ar = self.get_info_ci(ci, b"Slnt").unwrap_or_default();
                out.extend_from_slice(b"\n\t");
                out.extend_from_slice(&ar.short_src);
                out.push(b':');
                if ar.currentline > 0 {
                    out.extend_from_slice(ar.currentline.to_string().as_bytes());
                    out.push(b':');
                }
                out.extend_from_slice(b" in ");
                let nm = self.push_func_name_text(&ar);
                out.extend_from_slice(&nm);
                if ar.istailcall {
                    out.extend_from_slice(b"\n\t(...tail calls...)");
                }
            }
        }
        out
    }

    // =========================================================================================
    // loading
    // =========================================================================================

    /// `luaL_loadbufferx`: compile `chunk`; pushes the function or the error message.
    pub fn load_buffer(&mut self, chunk: &[u8], name: &[u8], mode: Option<&[u8]>) -> Status {
        self.load_chunk(chunk, name, mode)
    }

    /// `luaL_loadstring`.
    pub fn load_string(&mut self, s: &[u8]) -> Status {
        self.load_chunk(s, s, Some(b"t"))
    }

    /// `luaL_loadfilex`: `filename = None` reads standard input. File errors are reported with
    /// `Status::ErrSyntax` (the reference's `LUA_ERRFILE`) and the usual message.
    pub fn load_filex(&mut self, filename: Option<&[u8]>, mode: Option<&[u8]>) -> Status {
        use std::io::Read;
        let (chunkname, data): (Vec<u8>, Vec<u8>) = match filename {
            None => {
                let mut v = Vec::new();
                let r = std::io::stdin().lock().read_to_end(&mut v);
                if let Err(e) = r {
                    return self.errfile("read", b"stdin", &e);
                }
                (b"=stdin".to_vec(), v)
            }
            Some(f) => {
                let path = bytes_to_path(f);
                let mut file = match std::fs::File::open(&path) {
                    Ok(x) => x,
                    Err(e) => return self.errfile("open", f, &e),
                };
                let mut v = Vec::new();
                if let Err(e) = file.read_to_end(&mut v) {
                    return self.errfile("read", f, &e);
                }
                let mut n = b"@".to_vec();
                n.extend_from_slice(f);
                (n, v)
            }
        };
        let (skipped, c, pos) = skipcomment(&data);
        let mut buf: Vec<u8> = Vec::with_capacity(data.len() + 2);
        let (skipped, c, pos) = if c == Some(0x1b) {
            // binary file: no newline correction
            let _ = (skipped, pos);
            (false, c, pos)
        } else {
            (skipped, c, pos)
        };
        if skipped {
            buf.push(b'\n');
        }
        if let Some(c) = c {
            buf.push(c);
        }
        buf.extend_from_slice(&data[pos.min(data.len())..]);
        self.load_chunk(&buf, &chunkname, mode)
    }

    fn errfile(&mut self, what: &str, fname: &[u8], e: &std::io::Error) -> Status {
        let mut m = format!("cannot {what} ").into_bytes();
        m.extend_from_slice(fname);
        match e.raw_os_error() {
            Some(code) if code != 0 => {
                m.extend_from_slice(b": ");
                m.extend_from_slice(strerror(code).as_bytes());
            }
            _ => {}
        }
        match self.new_str_value(&m) {
            Ok(v) => {
                self.push(v);
                Status::ErrSyntax
            }
            Err(e) => e,
        }
    }

    // =========================================================================================
    // warnings
    // =========================================================================================

    /// `luaL_newstate`'s warning setup: install the reference warning function writing to stderr.
    /// `on` selects the initial state (5.4: off, 5.5: on).
    pub fn install_warn_function(&mut self, on: bool) {
        #[derive(PartialEq, Clone, Copy)]
        enum Mode {
            Off,
            On,
            Cont,
        }
        let mut mode = if on { Mode::On } else { Mode::Off };
        self.set_warn_function(Some(Box::new(move |msg: &[u8], tocont: bool| {
            use std::io::Write;
            let control = |mode: &mut Mode, msg: &[u8]| -> bool {
                if tocont || msg.first() != Some(&b'@') {
                    return false;
                }
                match &msg[1..] {
                    b"off" => *mode = Mode::Off,
                    b"on" => *mode = Mode::On,
                    _ => {}
                }
                true
            };
            match mode {
                Mode::Off => {
                    control(&mut mode, msg);
                }
                Mode::On | Mode::Cont => {
                    if mode == Mode::On {
                        if control(&mut mode, msg) {
                            return;
                        }
                        let _ = std::io::stderr().write_all(b"Lua warning: ");
                    }
                    let mut e = std::io::stderr();
                    let _ = e.write_all(msg);
                    if tocont {
                        mode = Mode::Cont;
                    } else {
                        let _ = e.write_all(b"\n");
                        mode = Mode::On;
                    }
                }
            }
        })));
    }
}

/// Skip a UTF-8 BOM and a first-line `#` comment like `skipcomment`/`skipBOM`: returns whether a
/// comment line was skipped, the first "valid" character (`None` = EOF) and the position after it.
fn skipcomment(data: &[u8]) -> (bool, Option<u8>, usize) {
    let mut pos = 0usize;
    let getc = |pos: &mut usize| -> Option<u8> {
        let c = data.get(*pos).copied();
        if c.is_some() {
            *pos += 1;
        }
        c
    };
    // skipBOM
    let mut c = getc(&mut pos);
    if c == Some(0xEF) && getc(&mut pos) == Some(0xBB) && getc(&mut pos) == Some(0xBF) {
        c = getc(&mut pos);
    } else if c == Some(0xEF) {
        // incomplete BOM: C consumed the extra characters it looked at; replicate the amount
        // (the first character is returned anyway)
        c = Some(0xEF);
    }
    if c == Some(b'#') {
        loop {
            let d = getc(&mut pos);
            if d.is_none() || d == Some(b'\n') {
                break;
            }
        }
        let c2 = getc(&mut pos);
        return (true, c2, pos);
    }
    (false, c, pos)
}

/// `strerror` text for an OS error code (without Rust's "(os error N)" suffix).
pub fn strerror(code: i32) -> String {
    let s = std::io::Error::from_raw_os_error(code).to_string();
    match s.rfind(" (os error") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// A byte string as a filesystem path (Unix: raw bytes).
pub fn bytes_to_path(b: &[u8]) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStrExt;
    std::path::PathBuf::from(std::ffi::OsStr::from_bytes(b))
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Integer conversion with floor/ceil modes is not needed by the libraries; this re-export keeps
/// `F2I` reachable for them.
pub use number::F2I as FloatToInt;

#[allow(unused)]
fn _keep(_: F2I) {}

/// `luaL_makeseed`: some randomness from the clock and an address.
pub fn make_seed() -> u32 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let local = 0u8;
    let addr = &local as *const u8 as usize as u64;
    let mut h = t ^ addr.rotate_left(17);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h as u32
}

impl State {
    /// `lua_getupvalue`: `(name, value)` of upvalue `n` (1-based) of function `f`.
    pub fn get_upvalue(&self, f: &Value, n: usize) -> Option<(Vec<u8>, Value)> {
        if n == 0 {
            return None;
        }
        if let Some(cl) = f.as_cclosure() {
            if n > cl.nupvalues() {
                return None;
            }
            return Some((Vec::new(), cl.upvalue(n - 1)));
        }
        if let Some(cl) = f.as_lclosure() {
            if n > cl.nupvalues() {
                return None;
            }
            let p = cl.proto();
            let name = p
                .upvalue_descs()
                .get(n - 1)
                .and_then(|u| u.name())
                .map(|s| s.get().as_bytes().to_vec())
                .unwrap_or_else(|| b"(no name)".to_vec());
            return Some((name, cl.upval(n - 1).get()));
        }
        None
    }

    /// `lua_setupvalue`: returns the upvalue name when it exists.
    pub fn set_upvalue_fn(&mut self, f: &Value, n: usize, v: Value) -> Option<Vec<u8>> {
        if n == 0 {
            return None;
        }
        if let Some(cl) = f.as_cclosure() {
            if n > cl.nupvalues() {
                return None;
            }
            cl.set_upvalue(self.heap(), n - 1, v);
            return Some(Vec::new());
        }
        if let Some(cl) = f.as_lclosure() {
            if n > cl.nupvalues() {
                return None;
            }
            let uv = cl.upval(n - 1);
            uv.set(v);
            self.heap().barrier(uv.as_ptr().cast(), &v);
            let name = cl
                .proto()
                .upvalue_descs()
                .get(n - 1)
                .and_then(|u| u.name())
                .map(|s| s.get().as_bytes().to_vec())
                .unwrap_or_else(|| b"(no name)".to_vec());
            return Some(name);
        }
        None
    }
}

impl State {
    /// A handle to another thread given its GC handle (`lua_tothread`). The handle proves the
    /// thread is alive; the logical rule "one `&mut State` per thread in use" is the caller's.
    #[allow(unsafe_code)]
    pub fn from_thread(t: Gc<Thread>) -> State {
        // SAFETY: `Gc<Thread>` is a live-thread handle (V1/ST1); `State` is itself only a handle.
        unsafe { State::from_gc(t) }
    }

    /// `lua_gethook` (the function pointer, if any).
    pub fn hook_fn(&self) -> Option<HookFn> {
        self.t().hook
    }

    /// `lua_gethookcount`.
    pub fn hook_count(&self) -> i32 {
        self.t().basehookcount
    }
}

impl State {
    /// `luaG_findlocal`: name and stack slot of local `n` of frame `ci` (negative `n` = varargs).
    pub fn find_local(&self, ci: Ci, n: i32) -> Option<(Vec<u8>, StkId)> {
        let base = ci.r().func + 1;
        let mut name: Option<Vec<u8>> = None;
        if ci.r().is_lua() {
            let cl = self.ci_lclosure(ci)?;
            let p = cl.proto();
            if n < 0 {
                if p.flag & crate::protodata::PF_VAHID != 0 {
                    let nextra = ci.r().nextraargs;
                    if n >= -nextra {
                        let pos = ci.r().func.0 as isize - nextra as isize - (n as isize + 1);
                        return Some((b"(vararg)".to_vec(), StkId(pos as usize)));
                    }
                }
                return None;
            }
            name = p
                .local_name(n, self.current_pc(ci))
                .map(|s| s.get().as_bytes().to_vec());
        }
        if name.is_none() {
            let limit = if ci == self.ci() {
                self.top()
            } else {
                ci.next()?.r().func
            };
            if n > 0 && limit.0 as isize - base.0 as isize >= n as isize {
                name = Some(if ci.r().is_lua() {
                    b"(temporary)".to_vec()
                } else {
                    b"(C temporary)".to_vec()
                });
            } else {
                return None;
            }
        }
        Some((name?, base + (n as usize - 1)))
    }

    /// `lua_upvalueid`: an identity token for upvalue `n` (1-based) of `f`.
    pub fn upvalue_id(&self, f: &Value, n: usize) -> Option<*mut std::ffi::c_void> {
        if n == 0 {
            return None;
        }
        if let Some(cl) = f.as_lclosure() {
            if n <= cl.nupvalues() {
                return Some(cl.upval(n - 1).as_ptr().cast());
            }
        } else if let Some(cl) = f.as_cclosure() {
            if n <= cl.nupvalues() {
                return Some(cl.as_ptr().cast::<u8>().wrapping_add(16 * n).cast());
            }
        }
        None
    }

    /// `lua_upvaluejoin`.
    pub fn upvalue_join(&mut self, f1: &Value, n1: usize, f2: &Value, n2: usize) {
        if let (Some(c1), Some(c2)) = (f1.as_lclosure(), f2.as_lclosure()) {
            let uv = c2.upval(n2 - 1);
            c1.set_upval(self.heap(), n1 - 1, uv);
        }
    }
}
