//! Library `string` (port of `lstrlib.c`): glue between the pure engines (`pattern`, `format`,
//! `pack`) and the State. Hot functions (`len`, `sub`, `byte`, `char`, `rep`, ...) avoid per-call
//! heap allocation by reusing a thread-local scratch buffer.
#![forbid(unsafe_code)]

#[cfg(not(rustynail_lua54))]
use rn_core::auxlib::MAX_SIZE as LUA_MAX_SIZE;
use rn_core::func::Proto;
use rn_core::number::{self, Number};
use rn_core::protodata::{Const, LocVar, ProtoData, UpvalDesc};
use rn_core::state::{State, StkId, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::string::StrRef;
use rn_core::value::{LuaType, Value};

use crate::format::{self, FormatError, FormatSource, Literal};
use crate::pack::{self, PackError, PackSource, Unpacked};
use crate::pattern::{self, Cap, MatchState, Replacement};

/// `luaL_checkstack`: ensure `n` free slots (also raises `ci.top` like `lua_checkstack`).
pub(crate) fn l_checkstack(st: &mut State, n: usize, msg: &str) -> LuaResult<()> {
    if st.stack_size() - st.top().0 <= n + 5 {
        match st.grow_stack(n, false) {
            Ok(true) => {}
            Ok(false) => {
                return Err(st.l_error(format!("stack overflow ({msg})").as_bytes()));
            }
            Err(e) => return Err(e),
        }
    }
    let need = st.top() + n;
    let ci = st.ci();
    if ci.r().top < need {
        ci.w().top = need;
    }
    Ok(())
}

/// `lua_checkstack` (no error): `true` if `n` extra slots are available.
pub(crate) fn can_checkstack(st: &mut State, n: usize) -> bool {
    if st.stack_size() - st.top().0 <= n + 5 && !matches!(st.grow_stack(n, false), Ok(true)) {
        return false;
    }
    let need = st.top() + n;
    let ci = st.ci();
    if ci.r().top < need {
        ci.w().top = need;
    }
    true
}

// ---------------------------------------------------------------------------------------------
// position helpers
// ---------------------------------------------------------------------------------------------

/// `posrelatI`.
#[inline]
fn posrelat_i(pos: i64, len: usize) -> usize {
    if pos > 0 {
        pos as usize
    } else if pos == 0 || pos < -(len as i64) {
        1
    } else {
        len.wrapping_add(pos as usize).wrapping_add(1)
    }
}

/// `getendpos`.
#[inline]
fn getendpos(st: &mut State, arg: usize, def: i64, len: usize) -> LuaResult<usize> {
    let pos = st.opt_integer(arg, def)?;
    Ok(if pos > len as i64 {
        len
    } else if pos >= 0 {
        pos as usize
    } else if pos < -(len as i64) {
        0
    } else {
        len.wrapping_add(pos as usize).wrapping_add(1)
    })
}

// ---------------------------------------------------------------------------------------------
// simple functions
// ---------------------------------------------------------------------------------------------

fn str_len(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?;
    st.push(Value::int(s.get().len() as i64));
    Ok(1)
}

fn str_sub(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    let l = s.len();
    let start = posrelat_i(st.check_integer(2)?, l);
    let end = getendpos(st, 3, -1, l)?;
    if start <= end {
        st.push_bytes(&s[start - 1..end])?;
    } else {
        st.push_bytes(b"")?;
    }
    Ok(1)
}

fn str_reverse(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    st.push_filled(s.len(), |out| {
        for (o, c) in out.iter_mut().zip(s.iter().rev().copied()) {
            o.write(c);
        }
    })?;
    Ok(1)
}

fn str_lower(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    st.push_filled(s.len(), |out| {
        for (o, c) in out.iter_mut().zip(s.iter().map(|c| c.to_ascii_lowercase())) {
            o.write(c);
        }
    })?;
    Ok(1)
}

fn str_upper(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    st.push_filled(s.len(), |out| {
        for (o, c) in out.iter_mut().zip(s.iter().map(|c| c.to_ascii_uppercase())) {
            o.write(c);
        }
    })?;
    Ok(1)
}

#[cfg(not(rustynail_lua54))]
const MAXSIZE: usize = LUA_MAX_SIZE;
#[cfg(rustynail_lua54)]
const MAXSIZE: usize = i32::MAX as usize;

fn str_rep(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    let n = st.check_integer(2)?;
    let sep: &[u8] = match st.opt_str(3)? {
        Some(x) => x.get().as_bytes(),
        None => b"",
    };
    let (len, lsep) = (s.len(), sep.len());
    #[cfg(not(rustynail_lua54))]
    let empty = n <= 0 || (len | lsep) == 0;
    #[cfg(rustynail_lua54)]
    let empty = n <= 0;
    if empty {
        st.push_bytes(b"")?;
        return Ok(1);
    }
    #[cfg(not(rustynail_lua54))]
    let too_large = len > MAXSIZE - lsep || (len + lsep) as i64 > (MAXSIZE as i64) / n;
    #[cfg(rustynail_lua54)]
    let too_large = len.wrapping_add(lsep) < len || (len + lsep) > MAXSIZE / (n as usize);
    if too_large {
        return Err(st.l_error(b"resulting string too large"));
    }
    #[cfg(not(rustynail_lua54))]
    let totallen = (n as usize) * (len + lsep) - lsep;
    #[cfg(rustynail_lua54)]
    let totallen = (n as usize) * len + (n as usize - 1) * lsep;
    #[cfg(rustynail_lua54)]
    if totallen == 0 {
        // both strings empty: the reference copies nothing `n` times
        st.push_bytes(b"")?;
        return Ok(1);
    }
    #[cfg(not(rustynail_lua54))]
    if totallen >= MAXSIZE {
        // luaL_prepbuffsize refuses sizes >= MAX_SIZE
        return Err(st.l_error(b"resulting string too large"));
    }
    st.push_filled(totallen, |out| {
        if lsep == 0 && len == 1 {
            for o in out.iter_mut() {
                o.write(s[0]);
            }
            return;
        }
        let mut pos = 0;
        let mut i = n;
        while i > 1 {
            for (o, c) in out[pos..pos + len].iter_mut().zip(s) {
                o.write(*c);
            }
            pos += len;
            for (o, c) in out[pos..pos + lsep].iter_mut().zip(sep) {
                o.write(*c);
            }
            pos += lsep;
            i -= 1;
        }
        for (o, c) in out[pos..].iter_mut().zip(s) {
            o.write(*c);
        }
    })?;
    Ok(1)
}

fn str_byte(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    let l = s.len();
    let pi = st.opt_integer(2, 1)?;
    let posi = posrelat_i(pi, l);
    let pose = getendpos(st, 3, pi, l)?;
    if posi > pose {
        return Ok(0);
    }
    if pose - posi >= i32::MAX as usize {
        return Err(st.l_error(b"string slice too long"));
    }
    let n = pose - posi + 1;
    l_checkstack(st, n, "string slice too long")?;
    for &c in &s[posi - 1..pose] {
        st.push(Value::int(c as i64));
    }
    Ok(n)
}

fn str_char(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let mut stack = [0u8; 64];
    let mut heap: Vec<u8>;
    let p: &mut [u8] = if n <= 64 {
        &mut stack[..n]
    } else {
        heap = vec![0u8; n];
        &mut heap[..]
    };
    for (i, slot) in p.iter_mut().enumerate() {
        let c = st.check_integer(i + 1)? as u64;
        if c > 255 {
            return Err(st.arg_error(i + 1, b"value out of range"));
        }
        *slot = c as u8;
    }
    st.push_bytes(p)?;
    Ok(1)
}

// ---------------------------------------------------------------------------------------------
// string.dump
// ---------------------------------------------------------------------------------------------

fn proto_to_data(p: &Proto) -> ProtoData<StrRef> {
    ProtoData {
        num_params: p.numparams,
        flags: p.flag,
        max_stack_size: p.maxstacksize,
        line_defined: p.linedefined,
        last_line_defined: p.lastlinedefined,
        code: p.code().to_vec(),
        k: p.k()
            .iter()
            .map(|v| {
                if let Some(i) = v.as_int() {
                    Const::Int(i)
                } else if let Some(f) = v.as_float() {
                    Const::Float(f)
                } else if let Some(s) = v.as_strref() {
                    Const::Str(s)
                } else if let Some(b) = v.as_bool() {
                    if b {
                        Const::True
                    } else {
                        Const::False
                    }
                } else {
                    Const::Nil
                }
            })
            .collect(),
        protos: p.protos().iter().map(|c| proto_to_data(c)).collect(),
        upvalues: p
            .upvalue_descs()
            .iter()
            .map(|u| UpvalDesc {
                name: u.name(),
                in_stack: u.instack,
                idx: u.idx,
                kind: u.kind,
            })
            .collect(),
        line_info: p.lineinfo().to_vec(),
        abs_line_info: p.abslineinfo().to_vec(),
        loc_vars: p
            .locvars()
            .iter()
            .map(|v| LocVar {
                name: v.name(),
                start_pc: v.startpc,
                end_pc: v.endpc,
            })
            .collect(),
        source: p.source(),
    }
}

/// Serialize the prototype of a Lua closure (`lua_dump`, `string.dump`).
pub fn dump_closure(
    st: &State,
    cl: rn_core::value::Gc<rn_core::func::LClosure>,
    strip: bool,
) -> Vec<u8> {
    let data = proto_to_data(&cl.proto());
    rn_compiler::dump(&data, st, strip)
}

fn str_dump(st: &mut State) -> LuaResult<usize> {
    let strip = st.arg(1).is_truthy();
    let f = st.arg(0);
    #[cfg(not(rustynail_lua54))]
    if st.nargs() < 1 || !f.is_function() || f.as_lclosure().is_none() {
        return Err(st.arg_error(1, b"Lua function expected"));
    }
    #[cfg(rustynail_lua54)]
    st.check_type(1, LuaType::Function)?;
    let Some(cl) = f.as_lclosure() else {
        return Err(st.l_error(b"unable to dump given function"));
    };
    let data = proto_to_data(&cl.proto());
    let out = rn_compiler::dump(&data, &*st, strip);
    st.push_vec(out)?;
    Ok(1)
}

// ---------------------------------------------------------------------------------------------
// string arithmetic metamethods
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Mod,
    Pow,
    Div,
    IDiv,
    Unm,
}

fn tonum(v: &Value) -> Option<Number> {
    State::to_number(v)
}

fn arith_op(op: Op, a: Number, b: Number) -> Result<Number, &'static str> {
    use Number::{Float, Int};
    let fa = |n: Number| match n {
        Int(i) => i as f64,
        Float(f) => f,
    };
    Ok(match op {
        Op::Add => match (a, b) {
            (Int(x), Int(y)) => Int(x.wrapping_add(y)),
            _ => Float(fa(a) + fa(b)),
        },
        Op::Sub => match (a, b) {
            (Int(x), Int(y)) => Int(x.wrapping_sub(y)),
            _ => Float(fa(a) - fa(b)),
        },
        Op::Mul => match (a, b) {
            (Int(x), Int(y)) => Int(x.wrapping_mul(y)),
            _ => Float(fa(a) * fa(b)),
        },
        Op::Mod => match (a, b) {
            (Int(x), Int(y)) => match number::imod(x, y) {
                Some(r) => Int(r),
                None => return Err("attempt to perform 'n%%0'"),
            },
            _ => Float(number::fmod_lua(fa(a), fa(b))),
        },
        Op::IDiv => match (a, b) {
            (Int(x), Int(y)) => match number::idiv(x, y) {
                Some(r) => Int(r),
                None => return Err("attempt to divide by zero"),
            },
            _ => Float(number::fidiv(fa(a), fa(b))),
        },
        Op::Pow => Float(number::pow_lua(fa(a), fa(b))),
        Op::Div => Float(fa(a) / fa(b)),
        Op::Unm => match b {
            Int(y) => Int(y.wrapping_neg()),
            Float(y) => Float(-y),
        },
    })
}

fn trymt(st: &mut State, mtkey: &[u8], opname: &[u8]) -> LuaResult<usize> {
    let a1 = st.arg(0);
    let a2 = st.arg(1);
    let mm = if a2.is_string() {
        Value::NIL
    } else {
        st.get_metafield(&a2, mtkey)
    };
    if mm.is_nil() {
        let mut m = b"attempt to ".to_vec();
        m.extend_from_slice(opname);
        m.extend_from_slice(b" a '");
        m.extend_from_slice(rn_core::auxlib::typename_of(&a1));
        m.extend_from_slice(b"' with a '");
        m.extend_from_slice(rn_core::auxlib::typename_of(&a2));
        m.push(b'\'');
        return Err(st.l_error(&m));
    }
    st.set_top(st.ci().r().func + 3); // exactly two arguments
    let r = st.call_value1(mm, &[a1, a2])?;
    st.push(r);
    Ok(1)
}

fn arith(st: &mut State, op: Op, mtname: &'static str) -> LuaResult<usize> {
    if let (Some(a), Some(b)) = (tonum(&st.arg(0)), tonum(&st.arg(1))) {
        match arith_op(op, a, b) {
            Ok(r) => {
                st.push(Value::number(r));
                Ok(1)
            }
            Err(m) => Err(st.run_error(m.replace("%%", "%").as_bytes())),
        }
    } else {
        trymt(st, mtname.as_bytes(), &mtname.as_bytes()[2..])
    }
}

fn arith_add(st: &mut State) -> LuaResult<usize> {
    arith(st, Op::Add, "__add")
}
fn arith_sub(st: &mut State) -> LuaResult<usize> {
    arith(st, Op::Sub, "__sub")
}
fn arith_mul(st: &mut State) -> LuaResult<usize> {
    arith(st, Op::Mul, "__mul")
}
fn arith_mod(st: &mut State) -> LuaResult<usize> {
    arith(st, Op::Mod, "__mod")
}
fn arith_pow(st: &mut State) -> LuaResult<usize> {
    arith(st, Op::Pow, "__pow")
}
fn arith_div(st: &mut State) -> LuaResult<usize> {
    arith(st, Op::Div, "__div")
}
fn arith_idiv(st: &mut State) -> LuaResult<usize> {
    arith(st, Op::IDiv, "__idiv")
}
fn arith_unm(st: &mut State) -> LuaResult<usize> {
    arith(st, Op::Unm, "__unm")
}

// ---------------------------------------------------------------------------------------------
// pattern matching
// ---------------------------------------------------------------------------------------------

fn push_cap(st: &mut State, c: &Cap<'_>) -> LuaResult<()> {
    match c {
        Cap::Str(b) => {
            st.push_bytes(b)?;
        }
        Cap::Pos(n) => st.push(Value::int(*n)),
    }
    Ok(())
}

fn str_find_aux(st: &mut State, find: bool) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    let p = st.check_str(2)?.get().as_bytes();
    let init = st.opt_integer(3, 1)?;
    let plain = find && st.arg(3).is_truthy();
    let r = pattern::find_with(s, p, init, find, plain, |ms, s1, end| -> LuaResult<usize> {
        let mut n = 0;
        if find {
            st.push(Value::int(s1 as i64 + 1));
            st.push(Value::int(end as i64));
            n = 2;
        }
        let Some(ms) = ms else {
            return Ok(n);
        };
        // `find` passes `s == NULL` to `push_captures`: no whole-match capture
        let whole = if find { (0, 0) } else { (s1, end) };
        let nc = ms.ncaptures(!find);
        push_captures(st, ms, nc, whole)?;
        Ok(n + nc)
    });
    match r {
        Err(m) => Err(st.l_error(&m)),
        Ok(None) => {
            st.push(Value::NIL);
            Ok(1)
        }
        Ok(Some(r)) => r,
    }
}

/// Push captures `0..nc` of a match spanning `whole` (`push_captures`).
fn push_captures(
    st: &mut State,
    ms: &MatchState<'_>,
    nc: usize,
    whole: (usize, usize),
) -> LuaResult<()> {
    l_checkstack(st, nc, "too many captures")?;
    for i in 0..nc {
        match ms.get_onecapture(i, whole.0, whole.1) {
            Ok(c) => push_cap(st, &c)?,
            Err(m) => return Err(st.l_error(&m)),
        }
    }
    Ok(())
}

fn str_find(st: &mut State) -> LuaResult<usize> {
    str_find_aux(st, true)
}

fn str_match(st: &mut State) -> LuaResult<usize> {
    str_find_aux(st, false)
}

fn gmatch_aux(st: &mut State) -> LuaResult<usize> {
    let func = st.get(st.ci().r().func);
    let Some(cl) = func.as_cclosure() else {
        return Ok(0);
    };
    let (Some(s), Some(p)) = (cl.upvalue(0).as_strref(), cl.upvalue(1).as_strref()) else {
        return Ok(0);
    };
    let (s, p) = (s.get().as_bytes(), p.get().as_bytes());
    let mut src = cl.upvalue(2).as_int().unwrap_or(0) as usize;
    let lastmatch = cl.upvalue(3).as_int().unwrap_or(-1);
    let mut ms = MatchState::new(s, p);
    let f = ms.start_filter();
    while src <= s.len() {
        src = ms.skip(f, src);
        ms.reprep();
        match ms.do_match(src, 0) {
            Err(m) => return Err(st.l_error(&m)),
            Ok(Some(e)) if e as i64 != lastmatch => {
                cl.set_upvalue_int(2, e as i64);
                cl.set_upvalue_int(3, e as i64);
                let nc = ms.ncaptures(true);
                push_captures(st, &ms, nc, (src, e))?;
                return Ok(nc);
            }
            _ => {}
        }
        src += 1;
    }
    cl.set_upvalue_int(2, s.len() as i64 + 1);
    Ok(0)
}

fn gmatch(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?;
    let p = st.check_str(2)?;
    let ls = s.get().len();
    let mut init = posrelat_i(st.opt_integer(3, 1)?, ls) - 1;
    if init > ls {
        init = ls + 1;
    }
    st.set_top(st.ci().r().func + 3);
    let cl = st.try_alloc(|h| h.new_cclosure(gmatch_aux, 4))?;
    st.push(Value::cclosure(cl));
    let heap = st.heap();
    cl.set_upvalue(heap, 0, Value::string(s));
    cl.set_upvalue(heap, 1, Value::string(p));
    cl.set_upvalue(heap, 2, Value::int(init as i64));
    cl.set_upvalue(heap, 3, Value::int(-1));
    st.check_gc();
    Ok(1)
}

fn str_gsub(st: &mut State) -> LuaResult<usize> {
    let src_s = st.check_str(1)?;
    let src = src_s.get().as_bytes();
    let pat = st.check_str(2)?.get().as_bytes();
    let tr = st.arg(2);
    let max_s = st.opt_integer(4, src.len() as i64 + 1)?;
    let ok =
        st.nargs() >= 3 && (tr.is_number() || tr.is_string() || tr.is_function() || tr.is_table());
    if !ok {
        return Err(st.type_error_arg(3, b"string/function/table"));
    }
    let mut err: Option<Status> = None;
    let mut out = st.take_scratch();
    let result = if tr.is_number() || tr.is_string() {
        let news = st.check_str(3)?.get().as_bytes();
        pattern::gsub_into(src, pat, Replacement::Str(news), max_s, &mut out)
    } else {
        let is_fn = tr.is_function();
        let mut cb = |m: &pattern::Matched<'_, '_>, out: &mut Vec<u8>| -> pattern::PResult<bool> {
            let r: Value = if is_fn {
                let nc = m.count();
                if st.check_stack(nc + 2).is_err() {
                    err = Some(Status::ErrRun);
                    return Err(Vec::new());
                }
                let func = st.top();
                st.push(tr);
                for i in 0..nc {
                    let c = match m.get(i) {
                        Ok(c) => c,
                        Err(msg) => {
                            st.set_top(func);
                            return Err(msg);
                        }
                    };
                    if let Err(e) = push_cap(st, &c) {
                        err = Some(e);
                        return Err(Vec::new());
                    }
                }
                if let Err(e) = st.call_no_yield(func, 1) {
                    err = Some(e);
                    return Err(Vec::new());
                }
                st.pop()
            } else {
                let key = match m.first()? {
                    Cap::Str(b) => match st.push_bytes(b) {
                        Ok(v) => v,
                        Err(e) => {
                            err = Some(e);
                            return Err(Vec::new());
                        }
                    },
                    Cap::Pos(n) => {
                        let v = Value::int(n);
                        st.push(v);
                        v
                    }
                };
                let r = st.index(tr, key);
                match r {
                    Ok(v) => {
                        st.pop();
                        v
                    }
                    Err(e) => {
                        err = Some(e);
                        return Err(Vec::new());
                    }
                }
            };
            if r.is_falsy() {
                Ok(false)
            } else if let Some(s) = r.as_string() {
                out.extend_from_slice(s.as_bytes());
                Ok(true)
            } else if let Some(n) = r.as_number() {
                number::with_tostring(n, |b| out.extend_from_slice(b));
                Ok(true)
            } else {
                let mut m = b"invalid replacement value (a ".to_vec();
                m.extend_from_slice(rn_core::auxlib::typename_of(&r));
                m.push(b')');
                Err(m)
            }
        };
        pattern::gsub_into(src, pat, Replacement::Append(&mut cb), max_s, &mut out)
    };
    match result {
        Err(m) => Err(match err {
            Some(e) => e,
            None => st.l_error(&m),
        }),
        Ok((changed, n)) => {
            if changed {
                st.push_scratch(out)?;
            } else {
                st.give_scratch(out);
                st.push(Value::string(src_s));
            }
            st.push(Value::int(n));
            Ok(2)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// string.format
// ---------------------------------------------------------------------------------------------

struct Fmt<'a> {
    st: &'a mut State,
    /// Position of the format string among the native's arguments minus one (0 for `string.format`).
    off: usize,
}

impl FormatSource for Fmt<'_> {
    type Error = Status;
    fn top(&self) -> usize {
        self.st.nargs() - self.off
    }
    fn check_integer(&mut self, arg: usize) -> Result<i64, Status> {
        self.st.check_integer(arg + self.off)
    }
    fn check_number(&mut self, arg: usize) -> Result<f64, Status> {
        self.st.check_number(arg + self.off)
    }
    fn tolstring(&mut self, arg: usize) -> Result<Vec<u8>, Status> {
        let v = self.st.arg(arg + self.off - 1);
        if let Some(s) = v.as_string() {
            if self.st.tostring_metafield(&v).is_nil() {
                return Ok(s.as_bytes().to_vec());
            }
        }
        let s = self.st.tolstring(v)?;
        let out = s.get().as_bytes().to_vec();
        self.st.pop();
        Ok(out)
    }
    fn tolstring_into(&mut self, arg: usize, out: &mut Vec<u8>) -> Result<(), Status> {
        let v = self.st.arg(arg + self.off - 1);
        if let Some(s) = v.as_string() {
            if self.st.tostring_metafield(&v).is_nil() {
                out.extend_from_slice(s.as_bytes());
                return Ok(());
            }
        }
        // numbers without a number metatable (the usual case): the text goes straight to `out`, no interned string
        if let Some(n) = v.as_number() {
            if self.st.metatable_of(&v).is_none() {
                number::with_tostring(n, |b| out.extend_from_slice(b));
                return Ok(());
            }
        }
        let s = self.st.tolstring(v)?;
        out.extend_from_slice(s.get().as_bytes());
        self.st.pop();
        Ok(())
    }
    fn literal(&mut self, arg: usize) -> Literal {
        let v = self.st.arg(arg + self.off - 1);
        if let Some(s) = v.as_string() {
            Literal::Str(s.as_bytes().to_vec())
        } else if let Some(i) = v.as_int() {
            Literal::Int(i)
        } else if let Some(f) = v.as_float() {
            Literal::Float(f)
        } else if v.is_nil() {
            Literal::Plain(b"nil".to_vec())
        } else if let Some(b) = v.as_bool() {
            Literal::Plain(if b {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            })
        } else {
            Literal::NoLiteral
        }
    }
    fn topointer(&mut self, arg: usize) -> Option<usize> {
        let v = self.st.arg(arg + self.off - 1);
        match self.st.value_pointer(&v) {
            0 => None,
            p => Some(p),
        }
    }
}

pub(crate) fn str_format(st: &mut State) -> LuaResult<usize> {
    format_at(st, 1)
}

/// `string.format` over the arguments starting at the format string `base` (1 for `string.format`; `buffer:putf`
/// passes 2): the result is pushed, argument errors name the real argument positions.
pub(crate) fn format_at(st: &mut State, base: usize) -> LuaResult<usize> {
    let off = base - 1;
    let fmt = st.check_str(base)?.get().as_bytes();
    let mut out = st.take_scratch();
    let r = format::format_into(fmt, &mut Fmt { st, off }, &mut out);
    match r {
        Ok(()) => {
            st.push_scratch(out)?;
            Ok(1)
        }
        Err(FormatError::Msg(m)) => Err(st.l_error(&m)),
        Err(FormatError::Arg(n, m)) => Err(st.arg_error(n + off, m.as_bytes())),
        Err(FormatError::Src(e)) => Err(e),
    }
}

// ---------------------------------------------------------------------------------------------
// pack / unpack
// ---------------------------------------------------------------------------------------------

struct PackSrc<'a> {
    st: &'a mut State,
}

impl PackSrc<'_> {
    /// The reference leaves a `nil` marker above the arguments: the first missing argument is
    /// reported as `got nil`, later ones as `got no value`.
    fn marker(&mut self, idx: usize) -> Option<Status> {
        if idx == self.st.nargs() + 1 {
            Some(self.st.arg_error(idx, b"number expected, got nil"))
        } else {
            None
        }
    }
}

impl PackSource for PackSrc<'_> {
    type Error = Status;
    fn int(&mut self, idx: usize) -> Result<i64, Status> {
        if let Some(e) = self.marker(idx) {
            return Err(e);
        }
        self.st.check_integer(idx)
    }
    fn num(&mut self, idx: usize) -> Result<f64, Status> {
        if let Some(e) = self.marker(idx) {
            return Err(e);
        }
        self.st.check_number(idx)
    }
    fn str(&mut self, idx: usize) -> Result<Vec<u8>, Status> {
        if idx == self.st.nargs() + 1 {
            return Err(self.st.arg_error(idx, b"string expected, got nil"));
        }
        Ok(self.st.check_str(idx)?.get().as_bytes().to_vec())
    }
}

fn pack_err<E>(st: &mut State, e: PackError<E>, src: impl FnOnce(E) -> Status) -> Status {
    match e {
        PackError::Msg(m) => st.l_error(&m),
        PackError::Arg(n, m) => st.arg_error(n, m.as_bytes()),
        PackError::Src(s) => src(s),
    }
}

fn str_pack(st: &mut State) -> LuaResult<usize> {
    let fmt = st.check_str(1)?.get().as_bytes();
    match pack::pack(fmt, &mut PackSrc { st }) {
        Ok(out) => {
            st.push_vec(out)?;
            Ok(1)
        }
        Err(e) => Err(pack_err(st, e, |s| s)),
    }
}

fn str_packsize(st: &mut State) -> LuaResult<usize> {
    let fmt = st.check_str(1)?.get().as_bytes();
    match pack::packsize(fmt) {
        Ok(n) => {
            st.push(Value::int(n as i64));
            Ok(1)
        }
        Err(e) => Err(pack_err(st, e, |s| match s {})),
    }
}

fn str_unpack(st: &mut State) -> LuaResult<usize> {
    let fmt = st.check_str(1)?.get().as_bytes();
    let data = st.check_str(2)?.get().as_bytes();
    let init = st.opt_integer(3, 1)?;
    match pack::unpack(fmt, data, init) {
        Ok((vals, next)) => {
            l_checkstack(st, vals.len() + 1, "too many results")?;
            for v in &vals {
                match v {
                    Unpacked::Int(i) => st.push(Value::int(*i)),
                    Unpacked::Num(f) => st.push(Value::float(*f)),
                    Unpacked::Str(b) => {
                        st.push_bytes(b)?;
                    }
                }
            }
            st.push(Value::int(next));
            Ok(vals.len() + 1)
        }
        Err(e) => Err(pack_err(st, e, |s| match s {})),
    }
}

// ---------------------------------------------------------------------------------------------
// registration
// ---------------------------------------------------------------------------------------------

/// `luaopen_string`.
pub fn open(st: &mut State) -> LuaResult<usize> {
    let lib = st.new_table_sized(0, 17)?;
    st.push(Value::table(lib));
    st.set_funcs(
        lib,
        &[
            ("byte", Some(str_byte)),
            ("char", Some(str_char)),
            ("dump", Some(str_dump)),
            ("find", Some(str_find)),
            ("format", Some(str_format)),
            ("gmatch", Some(gmatch)),
            ("gsub", Some(str_gsub)),
            ("len", Some(str_len)),
            ("lower", Some(str_lower)),
            ("match", Some(str_match)),
            ("rep", Some(str_rep)),
            ("reverse", Some(str_reverse)),
            ("sub", Some(str_sub)),
            ("upper", Some(str_upper)),
            ("pack", Some(str_pack)),
            ("packsize", Some(str_packsize)),
            ("unpack", Some(str_unpack)),
        ],
    )?;
    // metatable for strings
    let mt = st.new_table_sized(0, 9)?;
    st.push(Value::table(mt));
    st.set_funcs(
        mt,
        &[
            ("__add", Some(arith_add)),
            ("__sub", Some(arith_sub)),
            ("__mul", Some(arith_mul)),
            ("__mod", Some(arith_mod)),
            ("__pow", Some(arith_pow)),
            ("__div", Some(arith_div)),
            ("__idiv", Some(arith_idiv)),
            ("__unm", Some(arith_unm)),
        ],
    )?;
    st.raw_set_str(mt, b"__index", Value::table(lib))?;
    st.set_type_metatable(LuaType::String, Some(mt));
    st.pop(); // mt
    Ok(1)
}

#[allow(dead_code)]
fn _unused(_: StkId) -> i32 {
    LUA_MULTRET
}

/// F1: the address of `string.len`.
#[cfg(feature = "jit")]
pub fn jit_len_addr() -> usize {
    str_len as *const () as usize
}

/// BUNDLE-H (`libcall`): the addresses of `string.lower`, `string.upper`, `string.reverse` and `string.rep`.
#[cfg(feature = "jit")]
pub fn jit_str_fns() -> [usize; 4] {
    [str_lower as *const () as usize, str_upper as *const () as usize, str_reverse as *const () as usize, str_rep as *const () as usize]
}

/// BUNDLE-A: the address of `string.find`.
#[cfg(feature = "jit")]
pub fn jit_find_addr() -> usize {
    str_find as *const () as usize
}

/// BUNDLE-G1: the address of `string.format`.
#[cfg(feature = "jit")]
pub fn jit_format_addr() -> usize {
    str_format as *const () as usize
}

/// BUNDLE-C (`tfmt`): the argument kinds of a constant `string.format` format the trace helper can run: only `%%` and the items
/// `%[-0-9]*[dxXcs]` (at most five, at least one). `0` = an integer (`d x X c`), `1` = a string or an integer (`s`). `None` for any
/// other format (the call stays a stitching C call). The spec text itself is validated by running the reference engine
/// ([`jit_format`] at record time and at every run).
#[cfg(feature = "jit")]
pub fn jit_fmt_shape(fmt: &[u8]) -> Option<Vec<u8>> {
    if fmt.len() > 120 {
        return None;
    }
    let mut kinds = Vec::new();
    let mut i = 0;
    while i < fmt.len() {
        if fmt[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if *fmt.get(i)? == b'%' {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < fmt.len() && matches!(fmt[j], b'-' | b'0'..=b'9') {
            j += 1;
        }
        let k = match *fmt.get(j)? {
            b'd' | b'x' | b'X' | b'c' => 0,
            b's' => 1,
            _ => return None,
        };
        if kinds.len() == 5 {
            return None;
        }
        kinds.push(k);
        i = j + 1;
    }
    if kinds.is_empty() {
        None
    } else {
        Some(kinds)
    }
}

/// Registry of the constant formats of recorded traces (ids `0..JIT_FMT_MAX`): the helper finds the format bytes by id, so
/// the trace operands are only the arguments. The bytes are leaked copies at stable addresses (so the spec cache of
/// `format_into` hits on them).
#[cfg(feature = "jit")]
pub const JIT_FMT_MAX: usize = 64;
#[cfg(feature = "jit")]
static JIT_FMT: [std::sync::OnceLock<&'static [u8]>; JIT_FMT_MAX] = [const { std::sync::OnceLock::new() }; JIT_FMT_MAX];
#[cfg(feature = "jit")]
static JIT_FMT_N: std::sync::Mutex<usize> = std::sync::Mutex::new(0);

/// BUNDLE-C: the id of the format `fmt` in the registry (registered when new); `None` when the registry is full.
#[cfg(feature = "jit")]
pub fn jit_fmt_register(fmt: &[u8]) -> Option<usize> {
    let mut n = JIT_FMT_N.lock().unwrap_or_else(|e| e.into_inner());
    for id in 0..*n {
        if jit_fmt_get(id) == Some(fmt) {
            return Some(id);
        }
    }
    if *n >= JIT_FMT_MAX {
        return None;
    }
    let id = *n;
    let _ = JIT_FMT[id].set(Box::leak(fmt.to_vec().into_boxed_slice()));
    *n += 1;
    Some(id)
}

/// BUNDLE-C: the bytes of registered format `id`.
#[cfg(feature = "jit")]
pub fn jit_fmt_get(id: usize) -> Option<&'static [u8]> {
    JIT_FMT.get(id)?.get().copied()
}

/// BUNDLE-C (`tfmt`): `string.format(fmt, args...)` with the reference engine for arguments that are integers (for `d x X c`) and
/// strings or integers (for `s`): appends the result to `out`. `false` for anything that is not a plain success (an error the
/// library would raise, a `__tostring` of the string metatable, a number metatable, an argument of another kind): the caller then
/// lets the interpreter run the call. Nothing is allocated in the Lua heap.
#[cfg(feature = "jit")]
pub fn jit_format(st: &State, fmt: &[u8], args: &[Value], out: &mut Vec<u8>) -> bool {
    struct J<'a> {
        st: &'a State,
        args: &'a [Value],
    }
    impl FormatSource for J<'_> {
        type Error = ();
        fn top(&self) -> usize {
            self.args.len() + 1
        }
        fn check_integer(&mut self, arg: usize) -> Result<i64, ()> {
            self.args
                .get(arg.wrapping_sub(2))
                .and_then(Value::as_int)
                .ok_or(())
        }
        fn check_number(&mut self, _arg: usize) -> Result<f64, ()> {
            Err(())
        }
        fn tolstring(&mut self, arg: usize) -> Result<Vec<u8>, ()> {
            let mut v = Vec::new();
            self.tolstring_into(arg, &mut v)?;
            Ok(v)
        }
        fn tolstring_into(&mut self, arg: usize, out: &mut Vec<u8>) -> Result<(), ()> {
            let v = self.args.get(arg.wrapping_sub(2)).ok_or(())?;
            if let Some(s) = v.as_string() {
                if self.st.tostring_metafield(v).is_nil() {
                    out.extend_from_slice(s.as_bytes());
                    return Ok(());
                }
            } else if let Some(n) = v.as_int().map(Number::Int) {
                if self.st.metatable_of(v).is_none() {
                    number::with_tostring(n, |b| out.extend_from_slice(b));
                    return Ok(());
                }
            }
            Err(())
        }
        fn literal(&mut self, _arg: usize) -> Literal {
            Literal::NoLiteral
        }
        fn topointer(&mut self, _arg: usize) -> Option<usize> {
            None
        }
    }
    format::format_into(fmt, &mut J { st, args }, out).is_ok()
}

/// The address of the `gmatch` iterator function (`gmatch_aux`, the function of the C closure `gmatch` returns).
#[cfg(feature = "jit")]
pub fn jit_gmatch_addr() -> usize {
    gmatch_aux as *const () as usize
}

/// What [`jit_gmatch_step`] found.
#[cfg(feature = "jit")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GmStep {
    /// The step yields exactly one string (the whole match or one string capture) of this length (`with` accepted it; the state
    /// was updated when `commit` was set).
    Yield(usize),
    /// The iteration is over (the interpreter's step sets the position behind the subject and returns nothing; done when `commit` is set).
    End,
    /// Anything else (not a `gmatch` closure, several captures, a position capture, a pattern error, `with` refused): nothing changed.
    Other,
}

/// BUNDLE-GM: one step of the `gmatch` iterator closure `cl`, as `gmatch_aux` performs it, for the J5 `GMATCH` helper and its recorder.
/// A match yielding exactly one string value passes its bytes to `with`; when `with` answers true and `commit` is set the closure's
/// position and `lastmatch` upvalues advance exactly as `gmatch_aux` advances them. Errors (malformed pattern, too complex) and every
/// other shape change nothing and answer [`GmStep::Other`], so the interpreter runs the C function and raises them.
#[cfg(feature = "jit")]
pub fn jit_gmatch_step(cl: &Value, commit: bool, with: &mut dyn FnMut(&[u8]) -> bool) -> GmStep {
    let Some(c) = cl.as_cclosure() else {
        return GmStep::Other;
    };
    if c.f as usize != gmatch_aux as *const () as usize || c.nupvalues() != 4 {
        return GmStep::Other;
    }
    let (Some(s), Some(p)) = (c.upvalue(0).as_strref(), c.upvalue(1).as_strref()) else {
        return GmStep::Other;
    };
    let (s, p) = (s.get().as_bytes(), p.get().as_bytes());
    let mut src = c.upvalue(2).as_int().unwrap_or(0) as usize;
    let lastmatch = c.upvalue(3).as_int().unwrap_or(-1);
    let mut ms = MatchState::new(s, p);
    let f = ms.start_filter();
    while src <= s.len() {
        src = ms.skip(f, src);
        ms.reprep();
        match ms.do_match(src, 0) {
            Err(_) => return GmStep::Other,
            Ok(Some(e)) if e as i64 != lastmatch => {
                if ms.ncaptures(true) != 1 {
                    return GmStep::Other;
                }
                return match ms.get_onecapture(0, src, e) {
                    Ok(Cap::Str(b)) => {
                        if !with(b) {
                            return GmStep::Other;
                        }
                        if commit {
                            c.set_upvalue_int(2, e as i64);
                            c.set_upvalue_int(3, e as i64);
                        }
                        GmStep::Yield(b.len())
                    }
                    _ => GmStep::Other,
                };
            }
            _ => {}
        }
        src += 1;
    }
    if commit {
        // the interpreter's step does the same (and returns nothing): the next call finds the iteration over at once
        c.set_upvalue_int(2, s.len() as i64 + 1);
    }
    GmStep::End
}

/// P-B: the addresses of `string.sub`, `string.byte` and `string.char` (the J5 recorder runs them as intrinsics).
#[cfg(feature = "jit")]
pub fn jit_libfns() -> [usize; 3] {
    [
        str_sub as *const () as usize,
        str_byte as *const () as usize,
        str_char as *const () as usize,
    ]
}
