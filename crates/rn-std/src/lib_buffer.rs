//! `string.buffer`: a LuaJIT-compatible string buffer with the binary serialization format of
//! `buffer.encode` / `buffer.decode` (docs/EXTENSIONS.md). No FFI: the cdata methods (`reserve`, `commit`,
//! `putcdata`, `ref`) raise an error. A buffer is a userdata (like LuaJIT's `type(buf)`), its bytes live in
//! [`crate::buf_store`] and are charged to the state's memory limit.
//!
//! Wire format (verified byte for byte against LuaJIT 2.1): every item starts with an unsigned varint tag
//! `wu124` (1 byte below 0xe0, 2 bytes `0xe0 + (v >> 8)`, `v & 0xff` below 0x1fe0, else 0xff + u32 LE):
//! `0` nil, `1` false, `2` true, `6` int32 (decode only), `7` double (8 bytes LE), `8..=13` table
//! (`8 + hash? + 2 (array from index 0) | 4 (array from index 1)`, then the array count, then the hash count, then the array
//! values, then the key/value pairs), `0x0e` metatable-dictionary index + table, `0x0f` string-dictionary index (keys only),
//! `0x10` int64, `0x11` uint64, `0x20 + len` string.
#![forbid(unsafe_code)]

use crate::buf_store::{self, MAX_LEN};
use rn_core::number;
use rn_core::state::State;
use rn_core::status::{LuaResult, Status};
use rn_core::table::TableRef;
use rn_core::udata::Udata;
use rn_core::value::{Gc, Value};

const MT_NAME: &[u8] = b"string.buffer";
const UV_DICT: usize = 0;
const UV_MT: usize = 1;
const UV_DICT_REV: usize = 2;
const UV_MT_REV: usize = 3;
const NUV: usize = 4;
/// Table nesting limit of encode and decode (LuaJIT: 100).
const MAX_DEPTH: u32 = 100;

const TAG_NIL: u32 = 0;
const TAG_FALSE: u32 = 1;
const TAG_TRUE: u32 = 2;
const TAG_NULL: u32 = 3;
const TAG_LIGHTUD32: u32 = 4;
const TAG_LIGHTUD64: u32 = 5;
const TAG_INT: u32 = 6;
const TAG_NUM: u32 = 7;
const TAG_TAB: u32 = 8;
const TAG_DICT_MT: u32 = 0x0e;
const TAG_DICT_STR: u32 = 0x0f;
const TAG_INT64: u32 = 0x10;
const TAG_UINT64: u32 = 0x11;
const TAG_STR: u32 = 0x20;

// ---------------------------------------------------------------------------------------------
// the buffer object
// ---------------------------------------------------------------------------------------------

fn check_buf(st: &mut State, n: usize) -> LuaResult<Gc<Udata>> {
    if n <= st.nargs() {
        if let Some(u) = st.arg(n - 1).as_userdata() {
            let mt = st.registry_get(MT_NAME)?.as_table();
            if mt.is_some() && u.metatable() == mt {
                return Ok(u);
            }
        }
    }
    Err(st.type_error_arg(n, MT_NAME))
}

/// Make room for `extra` more bytes (charged to the memory limit); compacts the consumed prefix first.
fn ensure(st: &mut State, u: &Gc<Udata>, extra: usize) -> LuaResult<()> {
    let Some(b) = buf_store::inner(u) else {
        return Err(st.l_error(b"invalid buffer"));
    };
    if b.rpos > 0 && (b.rpos == b.data.len() || b.rpos >= 4096 && b.rpos >= b.data.len() / 2) {
        b.data.drain(..b.rpos);
        b.rpos = 0;
    }
    let len = b.data.len();
    let Some(need) = len.checked_add(extra).filter(|&n| n <= MAX_LEN) else {
        return Err(st.l_error(b"buffer overflow"));
    };
    let cap = b.data.capacity();
    if need <= cap {
        return Ok(());
    }
    let target = need.max(cap.saturating_mul(2)).clamp(64, MAX_LEN);
    let add = target.saturating_sub(b.charged);
    if !st.g_mut().heap.mem_mut().charge_ext(add) {
        return Err(Status::ErrMem);
    }
    let Some(b) = buf_store::inner(u) else {
        return Err(Status::ErrMem);
    };
    if b.data.try_reserve_exact(target - len).is_err() {
        st.g_mut().heap.mem_mut().uncharge_ext(add);
        return Err(Status::ErrMem);
    }
    b.charged += add;
    Ok(())
}

/// Append raw bytes.
fn append(st: &mut State, u: &Gc<Udata>, bytes: &[u8]) -> LuaResult<()> {
    ensure(st, u, bytes.len())?;
    match buf_store::inner(u) {
        Some(b) => {
            b.data.extend_from_slice(bytes);
            Ok(())
        }
        None => Err(st.l_error(b"invalid buffer")),
    }
}

fn unread(u: &Gc<Udata>) -> usize {
    buf_store::inner(u).map_or(0, |b| b.data.len() - b.rpos)
}

/// Run `f` over the byte vector, moved out of the buffer for the duration (the callee may allocate and so run
/// finalizers that touch this buffer: they see it empty, and what they add is appended afterwards). `f` gets the
/// bytes and the read position and returns how many bytes it consumed; the position advances only on success.
fn with_data(
    st: &mut State,
    u: &Gc<Udata>,
    f: impl FnOnce(&mut State, &[u8], usize) -> LuaResult<usize>,
) -> LuaResult<()> {
    let (data, rpos) = match buf_store::inner(u) {
        Some(b) => (std::mem::take(&mut b.data), b.rpos),
        None => return Err(st.l_error(b"invalid buffer")),
    };
    let r = f(st, &data, rpos);
    if let Some(b) = buf_store::inner(u) {
        let added = std::mem::replace(&mut b.data, data);
        if !added.is_empty() {
            b.data.extend_from_slice(&added);
        }
        if let Ok(used) = r {
            b.rpos += used;
            if b.rpos >= b.data.len() {
                b.data.clear();
                b.rpos = 0;
            }
        }
    }
    r.map(|_| ())
}

/// Convert a numeric argument to a byte count: `>= 0`, `< 2^31` (LuaJIT: "number out of range").
fn check_count(st: &mut State, n: usize) -> LuaResult<usize> {
    let x = st.check_number(n)?;
    if !(0.0..2147483648.0).contains(&x) {
        return Err(st.arg_error(n, b"number out of range"));
    }
    Ok(x as usize)
}

fn push_self(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    st.push(v);
    Ok(1)
}

/// `buffer.new([size|options [, options]])`.
fn buf_new(st: &mut State) -> LuaResult<usize> {
    let mut size = 0usize;
    let mut opts: Option<TableRef> = None;
    if st.nargs() >= 1 {
        let a = st.arg(0);
        if let Some(t) = a.as_table() {
            opts = Some(t);
        } else if !a.is_nil() {
            size = check_count(st, 1)?;
            if st.nargs() >= 2 {
                opts = st.arg(1).as_table();
            }
        } else if st.nargs() >= 2 {
            opts = st.arg(1).as_table();
        }
    }
    let u = st.new_udata(buf_store::PAYLOAD, NUV)?;
    buf_store::install(u);
    let uv = Value::userdata(u);
    st.push(uv);
    let mt = st.registry_get(MT_NAME)?.as_table();
    st.set_metatable(&uv, mt);
    if let Some(o) = opts {
        set_options(st, &u, o)?;
    }
    if size > 0 {
        ensure(st, &u, size)?;
    }
    st.check_gc();
    Ok(1)
}

/// `dict` (array of strings) and `metatable` (array of tables) options; the reverse maps are Lua tables kept in
/// user values of the buffer (so the collector sees them).
fn set_options(st: &mut State, u: &Gc<Udata>, o: TableRef) -> LuaResult<()> {
    for (field, uv, rev_uv, is_str) in [
        (&b"dict"[..], UV_DICT, UV_DICT_REV, true),
        (&b"metatable"[..], UV_MT, UV_MT_REV, false),
    ] {
        let arr = st.raw_get_str(o, field)?;
        let Some(arr) = arr.as_table() else { continue };
        // the entries up to the first nil (the same border for every Lua version: `#t` of a table with holes depends on the version's
        // `luaH_getn` -- `#{ "a", nil, "b" }` is 3 in 5.4 and 1 in 5.5 with the constructor's length hint -- and LuaJIT's dictionary stops at the hole)
        let mut n: i64 = 0;
        while !arr.get_int(n + 1).is_nil() {
            n += 1;
        }
        let rev = st.new_table_sized(0, n.min(1 << 20) as usize)?;
        st.push(Value::table(rev));
        for i in 1..=n {
            let v = arr.get_int(i);
            if v.is_string() != is_str || (!is_str && !v.is_table()) {
                return Err(st.l_error(b"bad options table"));
            }
            if rev.get(&v).is_nil() {
                st.raw_set(rev, &v, &Value::int(i - 1))?;
            }
        }
        let h = st.heap();
        u.set_user_value(h, rev_uv, Value::table(rev));
        let h = st.heap();
        u.set_user_value(h, uv, Value::table(arr));
        st.pop();
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// put / putf / set / get / skip / reset / free / tostring / len / concat
// ---------------------------------------------------------------------------------------------

fn buf_put(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    for i in 2..=st.nargs() {
        let v = st.arg(i - 1);
        if let Some(s) = v.as_string() {
            append(st, &u, s.as_bytes())?;
        } else if let Some(n) = v.as_number() {
            let b = number::tostring(n);
            append(st, &u, b.as_bytes())?;
        } else if !st.tostring_metafield(&v).is_nil() {
            let top = st.top();
            let s = st.tolstring(v)?;
            let bytes = s.get().as_bytes().to_vec();
            st.set_top(top);
            append(st, &u, &bytes)?;
        } else {
            let tn = st.arg_typename(i);
            let mut m = b"string/number/__tostring expected, got ".to_vec();
            m.extend_from_slice(tn);
            return Err(st.arg_error(i, &m));
        }
    }
    push_self(st)
}

fn buf_putf(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    crate::lib_string::format_at(st, 2)?;
    let r = st.pop();
    if let Some(s) = r.as_string() {
        append(st, &u, s.as_bytes())?;
    }
    push_self(st)
}

/// A string argument (numbers are not converted, like LuaJIT).
fn check_string(st: &mut State, n: usize) -> LuaResult<rn_core::string::StrRef> {
    match st.arg(n - 1).as_strref() {
        Some(s) if n <= st.nargs() => Ok(s),
        _ => Err(st.type_error_arg(n, b"string")),
    }
}

fn buf_set(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    let s = check_string(st, 2)?;
    if let Some(b) = buf_store::inner(&u) {
        b.data.clear();
        b.rpos = 0;
    }
    append(st, &u, s.get().as_bytes())?;
    push_self(st)
}

fn buf_reset(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    if let Some(b) = buf_store::inner(&u) {
        b.data.clear();
        b.rpos = 0;
    }
    push_self(st)
}

fn buf_free(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    let mut refund = 0;
    if let Some(b) = buf_store::inner(&u) {
        b.data = Vec::new();
        b.rpos = 0;
        refund = std::mem::take(&mut b.charged);
    }
    st.g_mut().heap.mem_mut().uncharge_ext(refund);
    push_self(st)
}

fn buf_skip(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    let n = check_count(st, 2)?;
    if let Some(b) = buf_store::inner(&u) {
        b.rpos = b.rpos.saturating_add(n).min(b.data.len());
        if b.rpos >= b.data.len() {
            b.data.clear();
            b.rpos = 0;
        }
    }
    push_self(st)
}

fn buf_get(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    let nargs = st.nargs();
    let mut lens: Vec<usize> = Vec::new();
    if nargs >= 2 && !st.arg(1).is_nil() {
        for i in 2..=nargs {
            lens.push(check_count(st, i)?);
        }
    }
    st.check_stack(lens.len().max(1) + 2)?;
    with_data(st, &u, |st, data, rpos| {
        let rest = &data[rpos..];
        if lens.is_empty() {
            st.push_bytes(rest)?;
            return Ok(rest.len());
        }
        let mut off = 0;
        for l in lens {
            let take = l.min(rest.len() - off);
            st.push_bytes(&rest[off..off + take])?;
            off += take;
        }
        Ok(off)
    })?;
    let n = if nargs >= 2 && !st.arg(1).is_nil() {
        nargs - 1
    } else {
        1
    };
    Ok(n)
}

fn buf_tostring(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    with_data(st, &u, |st, data, rpos| {
        st.push_bytes(&data[rpos..])?;
        Ok(0)
    })?;
    Ok(1)
}

fn buf_len(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    st.push(Value::int(unread(&u) as i64));
    Ok(1)
}

/// One operand of `..`: a buffer's unread bytes, a string, or a number.
fn concat_operand(st: &mut State, i: usize) -> LuaResult<Vec<u8>> {
    let v = st.arg(i - 1);
    if let Some(s) = v.as_string() {
        return Ok(s.as_bytes().to_vec());
    }
    if let Some(n) = v.as_number() {
        return Ok(number::tostring(n).as_bytes().to_vec());
    }
    if v.is_userdata() {
        if let Ok(u) = check_buf(st, i) {
            return Ok(buf_store::inner(&u).map_or_else(Vec::new, |b| b.data[b.rpos..].to_vec()));
        }
    }
    let mut m = b"attempt to concatenate a ".to_vec();
    m.extend_from_slice(st.arg_typename(i));
    m.extend_from_slice(b" value");
    Err(st.l_error(&m))
}

fn buf_concat(st: &mut State) -> LuaResult<usize> {
    let mut a = concat_operand(st, 1)?;
    let b = concat_operand(st, 2)?;
    if a.len().saturating_add(b.len()) > MAX_LEN {
        return Err(st.l_error(b"string length overflow"));
    }
    a.extend_from_slice(&b);
    st.push_bytes(&a)?;
    Ok(1)
}

fn buf_gc(st: &mut State) -> LuaResult<usize> {
    if let Some(u) = st.arg(0).as_userdata() {
        let refund = buf_store::drop_inner(u);
        st.g_mut().heap.mem_mut().uncharge_ext(refund);
    }
    Ok(0)
}

macro_rules! unsupported {
    ($name:ident, $msg:expr) => {
        fn $name(st: &mut State) -> LuaResult<usize> {
            Err(st.l_error($msg))
        }
    };
}
unsupported!(
    buf_reserve,
    b"string.buffer: 'reserve' is not supported (no FFI/cdata)"
);
unsupported!(
    buf_commit,
    b"string.buffer: 'commit' is not supported (no FFI/cdata)"
);
unsupported!(
    buf_putcdata,
    b"string.buffer: 'putcdata' is not supported (no FFI/cdata)"
);
unsupported!(
    buf_ref,
    b"string.buffer: 'ref' is not supported (no FFI/cdata)"
);

// ---------------------------------------------------------------------------------------------
// encode
// ---------------------------------------------------------------------------------------------

enum EncErr {
    Mem,
    Overflow,
    TooDeep,
    Unsupported(&'static [u8]),
}

struct Enc {
    out: Vec<u8>,
    dict_rev: Option<TableRef>,
    mt_rev: Option<TableRef>,
    depth: u32,
}

fn wu124(out: &mut Vec<u8>, v: u32) {
    if v < 0xe0 {
        out.push(v as u8);
    } else if v < 0xe0 + 0x1f00 {
        let w = v - 0xe0;
        out.push(0xe0 + (w >> 8) as u8);
        out.push(w as u8);
    } else {
        out.push(0xff);
        out.extend_from_slice(&v.to_le_bytes());
    }
}

impl Enc {
    fn room(&mut self, n: usize) -> Result<(), EncErr> {
        if self.out.len().saturating_add(n) > MAX_LEN {
            return Err(EncErr::Overflow);
        }
        self.out.try_reserve(n).map_err(|_| EncErr::Mem)
    }

    fn tag(&mut self, v: u32) -> Result<(), EncErr> {
        self.room(5)?;
        wu124(&mut self.out, v);
        Ok(())
    }

    fn put(&mut self, v: &Value, is_key: bool) -> Result<(), EncErr> {
        if v.is_nil() {
            return self.tag(TAG_NIL);
        }
        if let Some(b) = v.as_bool() {
            return self.tag(if b { TAG_TRUE } else { TAG_FALSE });
        }
        if let Some(i) = v.as_int() {
            if (-(1i64 << 53)..=(1i64 << 53)).contains(&i) {
                return self.put_num(i as f64);
            }
            self.tag(TAG_INT64)?;
            self.room(8)?;
            self.out.extend_from_slice(&i.to_le_bytes());
            return Ok(());
        }
        if let Some(f) = v.as_float() {
            return self.put_num(f);
        }
        if let Some(s) = v.as_string() {
            let b = s.as_bytes();
            if is_key {
                if let (Some(rev), Some(sr)) = (self.dict_rev, v.as_strref()) {
                    if let Some(idx) = rev.get(&Value::string(sr)).as_int() {
                        self.tag(TAG_DICT_STR)?;
                        return self.tag(idx as u32);
                    }
                }
            }
            if b.len() > (u32::MAX - TAG_STR) as usize {
                return Err(EncErr::Overflow);
            }
            self.tag(TAG_STR + b.len() as u32)?;
            self.room(b.len())?;
            self.out.extend_from_slice(b);
            return Ok(());
        }
        if let Some(t) = v.as_table() {
            return self.put_table(t);
        }
        if let Some(p) = v.as_light_userdata() {
            // light userdata: the address (LuaJIT serializes it the same way)
            let a = p as usize as u64;
            if a == 0 {
                return self.tag(TAG_NULL);
            }
            if a <= u64::from(u32::MAX) {
                self.tag(TAG_LIGHTUD32)?;
                self.room(4)?;
                self.out.extend_from_slice(&(a as u32).to_le_bytes());
            } else {
                self.tag(TAG_LIGHTUD64)?;
                self.room(8)?;
                self.out.extend_from_slice(&a.to_le_bytes());
            }
            return Ok(());
        }
        Err(EncErr::Unsupported(rn_core::auxlib::typename_of(v)))
    }

    fn put_num(&mut self, f: f64) -> Result<(), EncErr> {
        self.tag(TAG_NUM)?;
        self.room(8)?;
        self.out.extend_from_slice(&f.to_bits().to_le_bytes());
        Ok(())
    }

    fn put_table(&mut self, t: TableRef) -> Result<(), EncErr> {
        if self.depth == 0 {
            return Err(EncErr::TooDeep);
        }
        self.depth -= 1;
        if let (Some(mt), Some(rev)) = (t.metatable(), self.mt_rev) {
            if let Some(idx) = rev.get(&Value::table(mt)).as_int() {
                self.tag(TAG_DICT_MT)?;
                self.tag(idx as u32)?;
            }
        }
        // array part: indices 1..=n (n = highest non-nil index of the array part, extended over consecutive
        // integer keys living in the hash part), optionally preceded by index 0
        let asize = t.array_size() as i64;
        let mut n = (1..=asize)
            .rev()
            .find(|&i| !t.get_int(i).is_nil())
            .unwrap_or(0);
        while !t.get_int(n + 1).is_nil() {
            n += 1;
        }
        let has0 = !t.get_int(0).is_nil();
        let (narray, first) = if has0 {
            (n as u32 + 1, 0i64)
        } else if n > 0 {
            (n as u32 + 1, 1)
        } else {
            (0, 1)
        };
        let in_array =
            |k: &Value| -> bool { narray > 0 && k.as_int().is_some_and(|i| i >= first && i <= n) };
        let mut nhash = 0u32;
        let mut key = Value::NIL;
        while let Ok(Some((k, _))) = t.next(&key) {
            if !in_array(&k) {
                nhash += 1;
            }
            key = k;
        }
        let one = if has0 { 2 } else { 4 };
        self.tag(TAG_TAB + u32::from(nhash > 0) + if narray > 0 { one } else { 0 })?;
        if narray > 0 {
            self.tag(narray)?;
        }
        if nhash > 0 {
            self.tag(nhash)?;
        }
        if narray > 0 {
            for i in first..=n {
                self.put(&t.get_int(i), false)?;
            }
        }
        if nhash > 0 {
            let mut key = Value::NIL;
            while let Ok(Some((k, v))) = t.next(&key) {
                if !in_array(&k) {
                    self.put(&k, true)?;
                    self.put(&v, false)?;
                }
                key = k;
            }
        }
        self.depth += 1;
        Ok(())
    }
}

fn enc_error(st: &mut State, e: EncErr) -> Status {
    match e {
        EncErr::Mem => Status::ErrMem,
        EncErr::Overflow => st.l_error(b"buffer overflow"),
        EncErr::TooDeep => st.l_error(b"too deep to serialize"),
        EncErr::Unsupported(tn) => {
            let mut m = b"cannot serialize '".to_vec();
            m.extend_from_slice(tn);
            m.push(b'\'');
            st.l_error(&m)
        }
    }
}

fn encode_value(
    st: &mut State,
    v: &Value,
    dict_rev: Option<TableRef>,
    mt_rev: Option<TableRef>,
) -> LuaResult<Vec<u8>> {
    let mut e = Enc {
        out: Vec::new(),
        dict_rev,
        mt_rev,
        depth: MAX_DEPTH,
    };
    match e.put(v, false) {
        Ok(()) => Ok(e.out),
        Err(x) => Err(enc_error(st, x)),
    }
}

fn buf_encode(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    if st.nargs() < 2 {
        return Err(st.arg_error(2, b"value expected"));
    }
    let v = st.arg(1);
    let dict = u.user_value(UV_DICT_REV).and_then(|x| x.as_table());
    let mts = u.user_value(UV_MT_REV).and_then(|x| x.as_table());
    let bytes = encode_value(st, &v, dict, mts)?;
    append(st, &u, &bytes)?;
    push_self(st)
}

fn mod_encode(st: &mut State) -> LuaResult<usize> {
    if st.nargs() < 1 {
        return Err(st.arg_error(1, b"value expected"));
    }
    let v = st.arg(0);
    let bytes = encode_value(st, &v, None, None)?;
    st.push_bytes(&bytes)?;
    Ok(1)
}

// ---------------------------------------------------------------------------------------------
// decode
// ---------------------------------------------------------------------------------------------

struct Dec<'a> {
    data: &'a [u8],
    pos: usize,
    depth: u32,
    dict: Option<TableRef>,
    mts: Option<TableRef>,
}

const E_EOB: &[u8] = b"unexpected end of buffer";

impl Dec<'_> {
    fn byte(&mut self, st: &mut State) -> LuaResult<u8> {
        match self.data.get(self.pos) {
            Some(&b) => {
                self.pos += 1;
                Ok(b)
            }
            None => Err(st.l_error(E_EOB)),
        }
    }

    fn take(&mut self, st: &mut State, n: usize) -> LuaResult<&[u8]> {
        if self.data.len() - self.pos < n {
            return Err(st.l_error(E_EOB));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn wu124(&mut self, st: &mut State) -> LuaResult<u32> {
        let b = self.byte(st)?;
        if b < 0xe0 {
            Ok(u32::from(b))
        } else if b < 0xff {
            let lo = self.byte(st)?;
            Ok((((u32::from(b) - 0xe0) << 8) | u32::from(lo)) + 0xe0)
        } else {
            let s = self.take(st, 4)?;
            Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        }
    }

    /// Decode one value and push it.
    fn value(&mut self, st: &mut State) -> LuaResult<()> {
        let tag = self.wu124(st)?;
        if tag >= TAG_STR {
            let len = (tag - TAG_STR) as usize;
            let s = self.take(st, len)?;
            st.push_bytes(s)?;
            return Ok(());
        }
        match tag {
            TAG_NIL => st.push(Value::NIL),
            TAG_FALSE => st.push(Value::FALSE),
            TAG_TRUE => st.push(Value::TRUE),
            TAG_NULL => st.push(Value::light_userdata(std::ptr::null_mut())),
            TAG_LIGHTUD32 | TAG_LIGHTUD64 => {
                let n = if tag == TAG_LIGHTUD32 { 4 } else { 8 };
                let s = self.take(st, n)?;
                let mut b = [0u8; 8];
                b[..n].copy_from_slice(s);
                st.push(Value::light_userdata(
                    u64::from_le_bytes(b) as usize as *mut std::ffi::c_void
                ));
            }
            TAG_INT => {
                let s = self.take(st, 4)?;
                st.push(Value::int(i64::from(i32::from_le_bytes([
                    s[0], s[1], s[2], s[3],
                ]))));
            }
            TAG_NUM => {
                let s = self.take(st, 8)?;
                let f = f64::from_bits(u64::from_le_bytes(s.try_into().unwrap_or([0; 8])));
                st.push(num_value(f));
            }
            TAG_INT64 | TAG_UINT64 => {
                let s = self.take(st, 8)?;
                let bits = u64::from_le_bytes(s.try_into().unwrap_or([0; 8]));
                st.push(if tag == TAG_INT64 || bits <= i64::MAX as u64 {
                    Value::int(bits as i64)
                } else {
                    Value::float(bits as f64)
                });
            }
            TAG_DICT_STR => {
                let idx = self.wu124(st)?;
                match self.dict.map(|d| d.get_int(i64::from(idx) + 1)) {
                    Some(v) if v.is_string() => st.push(v),
                    _ => return Err(dict_err(st, idx)),
                }
            }
            TAG_DICT_MT => {
                let idx = self.wu124(st)?;
                let mt = match self.mts.map(|d| d.get_int(i64::from(idx) + 1)) {
                    Some(v) if v.is_table() => v,
                    _ => return Err(dict_err(st, idx)),
                };
                let next = self.data.get(self.pos).copied().map(u32::from);
                if !matches!(next, Some(8..=13)) {
                    return Err(st.l_error(b"cannot deserialize tag 0x0e"));
                }
                self.value(st)?;
                let tv = st.get(st.top() - 1);
                st.set_metatable(&tv, mt.as_table());
            }
            TAG_TAB..=13 => self.table(st, tag - TAG_TAB)?,
            _ => {
                let m = format!("cannot deserialize tag 0x{tag:02x}");
                return Err(st.l_error(m.as_bytes()));
            }
        }
        Ok(())
    }

    fn table(&mut self, st: &mut State, kind: u32) -> LuaResult<()> {
        if self.depth == 0 {
            return Err(st.l_error(b"too deep to serialize"));
        }
        let has_hash = kind & 1 != 0;
        let arr = kind >> 1; // 0 none, 1 from index 0, 2 from index 1
        let narray = if arr != 0 { self.wu124(st)? } else { 0 };
        let nhash = if has_hash { self.wu124(st)? } else { 0 };
        if narray > 1 << 28 || nhash > 1 << 26 {
            return Err(st.l_error(b"table overflow"));
        }
        // (the counts are untrusted: never presize beyond what the rest of the input could hold: every array
        // element takes at least one byte, every pair at least two; the error comes from the element that is missing)
        let remaining = self.data.len() - self.pos;
        let presize_arr = (narray as usize).min(remaining.saturating_add(1));
        let presize_hash = (nhash as usize).min(remaining / 2);
        st.check_stack(6)?;
        self.depth -= 1;
        let t = st.new_table_sized(presize_arr, presize_hash)?;
        st.push(Value::table(t));
        let first = if arr == 1 { 0 } else { 1 };
        for i in first..i64::from(narray) {
            self.value(st)?;
            let v = st.pop();
            st.raw_set_int(t, i, &v)?;
        }
        for _ in 0..nhash {
            self.value(st)?;
            // (LuaJIT rejects the key before it reads the value)
            let k = st.get(st.top() - 1);
            if k.is_nil() {
                return Err(st.l_error(b"table index is nil"));
            }
            if k.as_float().is_some_and(f64::is_nan) {
                return Err(st.l_error(b"table index is NaN"));
            }
            self.value(st)?;
            let v = st.pop();
            let k = st.pop();
            st.raw_set(t, &k, &v)?;
        }
        self.depth += 1;
        Ok(())
    }
}

fn dict_err(st: &mut State, idx: u32) -> Status {
    let m = format!("cannot deserialize dictionary index {}", u64::from(idx) + 1);
    st.l_error(m.as_bytes())
}

/// A double from the wire: integral values in +-2^53 become integers (so `{1, 2}` round-trips as integers); `-0.0`,
/// NaN, infinities and fractions stay floats.
fn num_value(f: f64) -> Value {
    if f == f.trunc() && f.abs() <= 9007199254740992.0 && !(f == 0.0 && f.is_sign_negative()) {
        Value::int(f as i64)
    } else {
        Value::float(f)
    }
}

fn decode_buf(
    st: &mut State,
    data: &[u8],
    dict: Option<TableRef>,
    mts: Option<TableRef>,
) -> LuaResult<usize> {
    let mut d = Dec {
        data,
        pos: 0,
        depth: MAX_DEPTH,
        dict,
        mts,
    };
    d.value(st)?;
    Ok(d.pos)
}

fn buf_decode(st: &mut State) -> LuaResult<usize> {
    let u = check_buf(st, 1)?;
    let dict = u.user_value(UV_DICT).and_then(|x| x.as_table());
    let mts = u.user_value(UV_MT).and_then(|x| x.as_table());
    with_data(st, &u, |st, data, rpos| {
        decode_buf(st, &data[rpos..], dict, mts)
    })?;
    Ok(1)
}

fn mod_decode(st: &mut State) -> LuaResult<usize> {
    let s = check_string(st, 1)?;
    let data = s.get().as_bytes().to_vec();
    let used = decode_buf(st, &data, None, None)?;
    if used != data.len() {
        return Err(st.l_error(b"left-over data in buffer"));
    }
    Ok(1)
}

// ---------------------------------------------------------------------------------------------
// module
// ---------------------------------------------------------------------------------------------

/// The `package.preload["string.buffer"]` loader: creates the metatable and the module table.
pub fn open(st: &mut State) -> LuaResult<usize> {
    let mt = match st.registry_get(MT_NAME)?.as_table() {
        Some(m) => m,
        None => {
            let mt = st.new_table_sized(0, 8)?;
            st.push(Value::table(mt));
            let nm = st.new_str_value(MT_NAME)?;
            st.raw_set_str(mt, b"__name", nm)?;
            st.set_funcs(
                mt,
                &[
                    ("__gc", Some(buf_gc)),
                    ("__tostring", Some(buf_tostring)),
                    ("__len", Some(buf_len)),
                    ("__concat", Some(buf_concat)),
                ],
            )?;
            let meth = st.new_table_sized(0, 16)?;
            st.push(Value::table(meth));
            st.set_funcs(
                meth,
                &[
                    ("put", Some(buf_put)),
                    ("putf", Some(buf_putf)),
                    ("putcdata", Some(buf_putcdata)),
                    ("set", Some(buf_set)),
                    ("reserve", Some(buf_reserve)),
                    ("commit", Some(buf_commit)),
                    ("skip", Some(buf_skip)),
                    ("get", Some(buf_get)),
                    ("tostring", Some(buf_tostring)),
                    ("encode", Some(buf_encode)),
                    ("decode", Some(buf_decode)),
                    ("reset", Some(buf_reset)),
                    ("free", Some(buf_free)),
                    ("ref", Some(buf_ref)),
                ],
            )?;
            st.raw_set_str(mt, b"__index", Value::table(meth))?;
            st.registry_set(MT_NAME, Value::table(mt))?;
            st.pop();
            st.pop();
            mt
        }
    };
    let _ = mt;
    let lib = st.new_table_sized(0, 3)?;
    st.push(Value::table(lib));
    st.set_funcs(
        lib,
        &[
            ("new", Some(buf_new)),
            ("encode", Some(mod_encode)),
            ("decode", Some(mod_decode)),
        ],
    )?;
    Ok(1)
}
