//! Error raising (the subset of `ldebug.c` that the runtime core needs: `luaG_runerror`,
//! `luaG_errormsg`, the `luaG_*error` family) plus `luaD_seterrorobj` / `luaD_errerr`.
//!
//! All raisers return the [`Status`] to put in `Err(..)`; the error *object* has then been pushed on
//! the stack (`State::error_value` reads it). `ErrMem` carries no object.
//!
//! Variable names in messages (`" (local 'x')"`) come from the [`crate::state::VarInfoFn`]
//! hook installed by the debug task; operands are identified by [`OpLoc`] (where the interpreter
//! found them), since values are passed by copy. Until the hook exists the suffix is empty.
use crate::number::{float_to_int, F2I};
use crate::state::*;
use crate::status::Status;
use crate::value::*;

/// Where an operand came from, for `varinfo`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpLoc {
    /// Unknown / temporary: no variable information.
    None,
    /// A stack slot of the running function.
    Stack(StkId),
    /// An upvalue of the running closure.
    Upvalue(u8),
    /// A constant of the running function.
    Constant(u32),
}

/// An operand value with its origin.
#[derive(Clone, Copy, Debug)]
pub struct Opnd {
    pub v: Value,
    pub loc: OpLoc,
}

impl From<Value> for Opnd {
    fn from(v: Value) -> Opnd {
        Opnd {
            v,
            loc: OpLoc::None,
        }
    }
}

impl Opnd {
    pub fn new(v: Value, loc: OpLoc) -> Opnd {
        Opnd { v, loc }
    }
}

const LUA_IDSIZE: usize = 60;

/// `luaO_chunkid`: printable chunk name from a `source` string.
pub fn chunk_id(source: &[u8]) -> Vec<u8> {
    let srclen = source.len();
    let mut out: Vec<u8> = Vec::new();
    match source.first() {
        Some(b'=') => {
            let rest = &source[1..];
            if srclen <= LUA_IDSIZE {
                out.extend_from_slice(rest);
            } else {
                out.extend_from_slice(&rest[..LUA_IDSIZE - 1]);
            }
        }
        Some(b'@') => {
            let rest = &source[1..];
            if srclen <= LUA_IDSIZE {
                out.extend_from_slice(rest);
            } else {
                out.extend_from_slice(b"...");
                let keep = LUA_IDSIZE - 3 - 1;
                out.extend_from_slice(&source[srclen - keep..]);
            }
        }
        _ => {
            const PRE: &[u8] = b"[string \"";
            const RETS: &[u8] = b"...";
            const POS: &[u8] = b"\"]";
            let nl = source.iter().position(|&c| c == b'\n');
            out.extend_from_slice(PRE);
            let bufflen = LUA_IDSIZE - (PRE.len() + RETS.len() + POS.len()) - 1;
            if srclen < bufflen && nl.is_none() {
                out.extend_from_slice(source);
            } else {
                let n = nl.unwrap_or(srclen).min(bufflen);
                out.extend_from_slice(&source[..n]);
                out.extend_from_slice(RETS);
            }
            out.extend_from_slice(POS);
        }
    }
    // C strings stop at an embedded NUL
    if let Some(z) = out.iter().position(|&c| c == 0) {
        out.truncate(z);
    }
    out
}

impl State {
    /// `luaD_seterrorobj`: move the error object (top-1) to `oldtop` (or the preallocated memory
    /// message for `ErrMem`) and set `top = oldtop + 1`.
    pub fn set_error_obj(&mut self, errcode: Status, oldtop: StkId) {
        if errcode == Status::ErrMem {
            let m = Value::string(self.g().memerrmsg);
            self.set(oldtop, m);
        } else {
            debug_assert!(matches!(
                errcode,
                Status::ErrRun | Status::ErrSyntax | Status::ErrErr
            ));
            let e = self.get(self.top() - 1);
            self.set(oldtop, e);
        }
        self.set_top(oldtop + 1);
    }

    /// The error object of the error being propagated (valid right after an `Err(..)` other than
    /// `ErrMem`, until the stack is modified).
    pub fn error_value(&self) -> Value {
        self.get(self.top() - 1)
    }

    /// Throw the value `v` as a Lua error (`lua_error`): pushes it, runs the message handler and
    /// returns `ErrRun` (or the status of a failing handler).
    pub fn throw_value(&mut self, v: Value) -> Status {
        if let Err(e) = self.check_stack(1) {
            return e;
        }
        self.push(v);
        self.error_msg()
    }

    /// `luaG_errormsg`: the error object is on top of the stack; call the message handler (if any).
    pub fn error_msg(&mut self) -> Status {
        let ef = self.errfunc();
        if ef != 0 {
            let handler = self.get(StkId(ef));
            let top = self.top();
            let arg = self.get(top - 1);
            self.set(top, arg);
            self.set(top - 1, handler);
            self.set_top(top + 1); // EXTRA_STACK
            if let Err(e) = self.call_no_yield(top - 1, 1) {
                return e;
            }
        }
        #[cfg(not(rustynail_lua54))]
        if self.get(self.top() - 1).is_nil() {
            match self.new_str_value(b"<no error object>") {
                Ok(s) => {
                    let t = self.top() - 1;
                    self.set(t, s);
                }
                Err(e) => return e,
            }
        }
        Status::ErrRun
    }

    /// `luaD_errerr`: "error in error handling".
    pub fn err_err(&mut self) -> Status {
        match self.new_str_value(b"error in error handling") {
            Ok(s) => {
                self.push(s); // assume EXTRA_STACK
                Status::ErrErr
            }
            Err(e) => e,
        }
    }

    /// `luaE_checkcstack`.
    pub fn check_cstack(&mut self) -> Result<(), Status> {
        let n = self.ccalls();
        if n == LUAI_MAXCCALLS {
            Err(self.run_error(b"C stack overflow"))
        } else if n >= LUAI_MAXCCALLS / 10 * 11 {
            Err(self.err_err())
        } else {
            Ok(())
        }
    }

    /// `luaE_incCstack`.
    pub fn inc_cstack(&mut self) -> Result<(), Status> {
        self.tm().ncalls += 1;
        if self.ccalls() >= LUAI_MAXCCALLS {
            self.check_cstack()?;
        }
        Ok(())
    }

    /// `luaG_addinfo`: `chunk:line: msg`.
    pub fn add_info(&self, msg: &[u8], source: Option<&[u8]>, line: i32) -> Vec<u8> {
        let mut out = match source {
            #[cfg(not(rustynail_lua54))]
            None => b"?:?".to_vec(),
            // 5.4 prints the line even without a source (-1 without line info)
            #[cfg(rustynail_lua54)]
            None => format!("?:{line}").into_bytes(),
            Some(src) => {
                let mut o = chunk_id(src);
                o.push(b':');
                o.extend_from_slice(line.to_string().as_bytes());
                o
            }
        };
        out.extend_from_slice(b": ");
        out.extend_from_slice(msg);
        out
    }

    /// Current line of a Lua frame (`getcurrentline`).
    pub fn current_line(&self, ci: Ci) -> i32 {
        match self.ci_lclosure(ci) {
            Some(cl) => cl.proto().line_at(self.current_pc(ci)),
            None => -1,
        }
    }

    /// `luaG_runerror`: format-free variant. Adds `source:line:` when the running function is a Lua
    /// function, runs the message handler and returns the status to propagate.
    #[cold]
    pub fn run_error(&mut self, msg: &[u8]) -> Status {
        self.check_gc();
        let ci = self.ci();
        let full: Vec<u8> = if ci.r().is_lua() {
            let (src, line) = match self.ci_lclosure(ci) {
                Some(cl) => {
                    let p = cl.proto();
                    (
                        p.source().map(|s| s.get().as_bytes().to_vec()),
                        p.line_at(self.current_pc(ci)),
                    )
                }
                None => (None, -1),
            };
            self.add_info(msg, src.as_deref(), line)
        } else {
            msg.to_vec()
        };
        match self.new_str_value(&full) {
            Ok(s) => {
                if let Err(e) = self.check_stack(1) {
                    return e;
                }
                self.push(s);
                self.error_msg()
            }
            Err(e) => e,
        }
    }

    /// Run [`State::run_error`] and wrap it as `Err` (convenience).
    pub fn rt_error<T>(&mut self, msg: &[u8]) -> Result<T, Status> {
        Err(self.run_error(msg))
    }

    pub(crate) fn var_info(&self, loc: OpLoc) -> Vec<u8> {
        if loc == OpLoc::None {
            return Vec::new();
        }
        (self.g().varinfo)(self, loc).unwrap_or_default()
    }

    /// `luaG_typeerror`: `"attempt to <op> a <type> value<varinfo>"`.
    #[cold]
    pub fn type_error(&mut self, o: Opnd, op: &str) -> Status {
        let extra = self.var_info(o.loc);
        self.type_error_extra(&o.v, op, &extra)
    }

    fn type_error_extra(&mut self, v: &Value, op: &str, extra: &[u8]) -> Status {
        let mut m = b"attempt to ".to_vec();
        m.extend_from_slice(op.as_bytes());
        m.extend_from_slice(b" a ");
        m.extend_from_slice(self.obj_typename(v));
        m.extend_from_slice(b" value");
        m.extend_from_slice(extra);
        self.run_error(&m)
    }

    /// `luaG_callerror`: the called object is named after the calling instruction when possible
    /// (`funcnamefromcall`), else after the variable that held it.
    #[cold]
    pub fn call_error(&mut self, o: Opnd) -> Status {
        let ci = self.ci();
        let fallback = self.var_info(o.loc);
        let extra = if ci.r().callstatus & CIST_HOOKED != 0 {
            crate::debuginfo::format_varinfo(Some(crate::debuginfo::funcname_hook()))
        } else if ci.r().callstatus & CIST_FIN != 0 {
            crate::debuginfo::format_varinfo(Some(crate::debuginfo::funcname_finalizer()))
        } else if let Some(cl) = self.ci_lclosure(ci) {
            let pc = self.current_pc(ci);
            if pc >= 0 {
                crate::debuginfo::callerror_extra(&*cl.proto(), pc as usize, fallback)
            } else {
                fallback
            }
        } else {
            fallback
        };
        self.type_error_extra(&o.v, "call", &extra)
    }

    /// `luaG_concaterror`.
    pub fn concat_error(&mut self, p1: Opnd, p2: Opnd) -> Status {
        let bad = if p1.v.is_string() || p1.v.is_number() {
            p2
        } else {
            p1
        };
        self.type_error(bad, "concatenate")
    }

    /// `luaG_opinterror`.
    pub fn opint_error(&mut self, p1: Opnd, p2: Opnd, msg: &str) -> Status {
        let bad = if !p1.v.is_number() { p1 } else { p2 };
        self.type_error(bad, msg)
    }

    /// `luaG_tointerror`.
    pub fn toint_error(&mut self, p1: Opnd, p2: Opnd) -> Status {
        let bad = if tointeger_ns(&p1.v).is_none() {
            p1
        } else {
            p2
        };
        let extra = self.var_info(bad.loc);
        let mut m = b"number".to_vec();
        m.extend_from_slice(&extra);
        m.extend_from_slice(b" has no integer representation");
        self.run_error(&m)
    }

    /// `luaG_ordererror`.
    pub fn order_error(&mut self, p1: &Value, p2: &Value) -> Status {
        let t1 = self.obj_typename(p1).to_vec();
        let t2 = self.obj_typename(p2).to_vec();
        let mut m = Vec::new();
        if t1 == t2 {
            m.extend_from_slice(b"attempt to compare two ");
            m.extend_from_slice(&t1);
            m.extend_from_slice(b" values");
        } else {
            m.extend_from_slice(b"attempt to compare ");
            m.extend_from_slice(&t1);
            m.extend_from_slice(b" with ");
            m.extend_from_slice(&t2);
        }
        self.run_error(&m)
    }

    /// `luaG_forerror`: `"bad 'for' <what> (number expected, got <type>)"` (5.4.9 and 5.5).
    pub fn for_error(&mut self, o: &Value, what: &str) -> Status {
        let mut m = b"bad 'for' ".to_vec();
        m.extend_from_slice(what.as_bytes());
        m.extend_from_slice(b" (number expected, got ");
        m.extend_from_slice(self.obj_typename(o));
        m.push(b')');
        self.run_error(&m)
    }

    // ---- minimal frame introspection (the debug task replaces/extends this) -----------------

    /// Closure running in a Lua frame.
    pub fn ci_lclosure(&self, ci: Ci) -> Option<Gc<crate::func::LClosure>> {
        if ci.r().is_lua() {
            self.get(ci.r().func).as_lclosure()
        } else {
            None
        }
    }

    /// `pcRel(ci->u.l.savedpc, p)` (index of the executing instruction).
    pub fn current_pc(&self, ci: Ci) -> i32 {
        match self.ci_lclosure(ci) {
            Some(cl) => {
                let code = cl.proto().code_ptr();
                // SAFETY: `savedpc` points into (or one past) the proto's code array.
                unsafe { ci.r().savedpc.offset_from(code) as i32 - 1 }
            }
            None => -1,
        }
    }

    /// Name of the `n`-th (1-based) local of a frame, `"(temporary)"`/`"(C temporary)"` for other
    /// valid slots (the subset of `luaG_findlocal` that `checkclosemth` needs).
    pub fn find_local_name(&self, ci: Ci, n: i32) -> Option<Vec<u8>> {
        let mut name: Option<Vec<u8>> = None;
        if let Some(cl) = self.ci_lclosure(ci) {
            if n >= 0 {
                name = cl
                    .proto()
                    .local_name(n, self.current_pc(ci))
                    .map(|s| s.get().as_bytes().to_vec());
            }
        }
        if name.is_none() {
            let base = ci.r().func + 1;
            let limit = if ci == self.ci() {
                self.top()
            } else {
                ci.next().map_or(self.top(), |nx| nx.r().func)
            };
            if n > 0 && (limit.0 as isize - base.0 as isize) >= n as isize {
                name = Some(if ci.r().is_lua() {
                    b"(temporary)".to_vec()
                } else {
                    b"(C temporary)".to_vec()
                });
            }
        }
        name
    }
}

/// `luaV_tointegerns` with `LUA_FLOORN2I` == exact only: integers, floats with an exact integer
/// value, and strings convertible to such numbers.
pub fn tointeger_ns(v: &Value) -> Option<i64> {
    if let Some(i) = v.as_int() {
        return Some(i);
    }
    if let Some(f) = v.as_float() {
        return float_to_int(f, F2I::Eq);
    }
    if let Some(s) = v.as_string() {
        return match crate::number::str2num(s.as_bytes())? {
            crate::number::Number::Int(i) => Some(i),
            crate::number::Number::Float(f) => float_to_int(f, F2I::Eq),
        };
    }
    None
}

/// The default [`crate::state::VarInfoFn`]: names registers and upvalues of the running
/// Lua function with the symbolic execution of [`crate::debuginfo`] (`varinfo` in `ldebug.c`).
pub fn default_varinfo(st: &State, loc: OpLoc) -> Option<Vec<u8>> {
    let ci = st.ci();
    let cl = st.ci_lclosure(ci)?;
    let p = cl.proto();
    let pc = st.current_pc(ci);
    match loc {
        OpLoc::Stack(i) => {
            let base = ci.r().func + 1;
            if i.0 < base.0 || i.0 >= ci.r().top.0 || pc < 0 {
                return None;
            }
            let v = crate::debuginfo::varinfo_register(&*p, pc as usize, i.0 - base.0);
            Some(v)
        }
        OpLoc::Upvalue(idx) => Some(crate::debuginfo::varinfo_upvalue(&*p, idx as usize)),
        OpLoc::Constant(_) | OpLoc::None => None,
    }
}
