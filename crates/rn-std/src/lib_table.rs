//! Library `table` (port of `ltablib.c`), including the reference quicksort so that comparison
//! call order and the "invalid order function" behaviour match.
#![forbid(unsafe_code)]

use rn_core::number;
use rn_core::state::State;
use rn_core::status::LuaResult;
use rn_core::value::{LuaType, Value};

use crate::lib_string::{can_checkstack, l_checkstack};

const TAB_R: u8 = 1;
const TAB_W: u8 = 2;
const TAB_L: u8 = 4;
const TAB_RW: u8 = TAB_R | TAB_W;

/// Does the metatable of `v` have a non-nil raw field `key`?
fn checkfield(st: &mut State, mt: rn_core::table::TableRef, key: &[u8]) -> bool {
    match st.raw_get_str(mt, key) {
        Ok(v) => !v.is_nil(),
        Err(_) => false,
    }
}

/// `checktab`: the argument is a table or behaves like one.
fn checktab(st: &mut State, arg: usize, what: u8) -> LuaResult<()> {
    let v = st.arg(arg - 1);
    if arg <= st.nargs() && v.is_table() {
        return Ok(());
    }
    if arg <= st.nargs() {
        if let Some(mt) = st.metatable_of(&v) {
            if (what & TAB_R == 0 || checkfield(st, mt, b"__index"))
                && (what & TAB_W == 0 || checkfield(st, mt, b"__newindex"))
                // (5.5: strings don't need '__len' to have a length)
                && (what & TAB_L == 0
                    || (v.is_string() && !cfg!(rustynail_lua54))
                    || checkfield(st, mt, b"__len"))
            {
                return Ok(());
            }
        }
    }
    st.check_type(arg, LuaType::Table).map(|_| ())
}

/// `aux_getn`.
fn aux_getn(st: &mut State, n: usize, w: u8) -> LuaResult<i64> {
    checktab(st, n, w | TAB_L)?;
    let v = st.arg(n - 1);
    if let Some(t) = v.as_table() {
        if t.metatable().is_none() {
            return Ok(t.len(&st.g().heap) as i64);
        }
    }
    st.len_int(v)
}

#[cfg(not(rustynail_lua54))]
fn tcreate(st: &mut State) -> LuaResult<usize> {
    let sizeseq = st.check_integer(1)? as u64;
    let sizerest = st.opt_integer(2, 0)? as u64;
    st.arg_check(sizeseq <= i32::MAX as u64, 1, b"out of range")?;
    st.arg_check(sizerest <= i32::MAX as u64, 2, b"out of range")?;
    let t = st.new_table_sized(sizeseq as usize, sizerest as usize)?;
    st.push(Value::table(t));
    Ok(1)
}

fn tinsert(st: &mut State) -> LuaResult<usize> {
    let tv = st.arg(0);
    let mut e = aux_getn(st, 1, TAB_RW)?;
    e = e.wrapping_add(1); // first empty element
    let pos;
    match st.nargs() {
        2 => pos = e,
        3 => {
            pos = st.check_integer(2)?;
            st.arg_check(
                (pos as u64).wrapping_sub(1) < e as u64,
                2,
                b"position out of bounds",
            )?;
            let mut i = e;
            if let Some(t) = tv.as_table() {
                // N2: the slot `e` may lie past the array part (the table grows there): one
                // regular step, then the rest as a bulk move when the array covers it
                if i > pos && i as u64 > u64::from(t.array_size()) {
                    let v = st.index_int(tv, i - 1)?;
                    st.push(v);
                    let r = st.set_index_int(tv, i, v);
                    r?;
                    st.pop();
                    i -= 1;
                }
                if i > pos && t.arr_move(pos, pos + 1, (i - pos) as usize, true) {
                    i = pos;
                }
            }
            while i > pos {
                let v = st.index_int(tv, i - 1)?;
                st.push(v);
                let r = st.set_index_int(tv, i, v);
                r?;
                st.pop();
                i -= 1;
            }
        }
        _ => return Err(st.l_error(b"wrong number of arguments to 'insert'")),
    }
    let v = st.arg(st.nargs() - 1);
    st.set_index_int(tv, pos, v)?;
    Ok(0)
}

fn tremove(st: &mut State) -> LuaResult<usize> {
    let tv = st.arg(0);
    let size = aux_getn(st, 1, TAB_RW)?;
    let mut pos = st.opt_integer(2, size)?;
    if pos != size {
        st.arg_check(
            (pos as u64).wrapping_sub(1) <= size as u64,
            2,
            b"position out of bounds",
        )?;
    }
    let res = st.index_int(tv, pos)?;
    st.push(res);
    if pos < size {
        // N2: bulk move of the tail when the array part covers it (no metamethod can fire)
        if let Some(t) = tv.as_table() {
            if t.arr_move(pos + 1, pos, (size - pos) as usize, false) {
                pos = size;
            }
        }
    }
    while pos < size {
        let v = st.index_int(tv, pos + 1)?;
        st.push(v);
        let r = st.set_index_int(tv, pos, v);
        r?;
        st.pop();
        pos += 1;
    }
    st.set_index_int(tv, pos, Value::NIL)?;
    Ok(1)
}

fn tmove(st: &mut State) -> LuaResult<usize> {
    let f = st.check_integer(2)?;
    let e = st.check_integer(3)?;
    let t = st.check_integer(4)?;
    let tt = if !st.arg_is_none_or_nil(5) { 5 } else { 1 };
    checktab(st, 1, TAB_R)?;
    checktab(st, tt, TAB_W)?;
    let src = st.arg(0);
    let dst = st.arg(tt - 1);
    if e >= f {
        st.arg_check(f > 0 || e < i64::MAX + f, 3, b"too many elements to move")?;
        let n = e - f + 1;
        st.arg_check(t <= i64::MAX - n + 1, 4, b"destination wrap around")?;
        let fwd = t > e || t <= f || (tt != 1 && !st.equals(src, dst)?);
        // N2: same table (identical object), both ranges in its array part: one `memmove`
        let done = match (src.as_table(), dst.as_table()) {
            (Some(a), Some(b)) if a == b && f > 0 && t > 0 => a.arr_move(f, t, n as usize, !fwd),
            (Some(a), Some(b)) if f > 0 && t > 0 => a.arr_copy_to(st.heap(), b, f, t, n as usize),
            _ => false,
        };
        if done {
        } else if fwd {
            for i in 0..n {
                let v = st.index_int(src, f + i)?;
                st.push(v);
                let r = st.set_index_int(dst, t + i, v);
                r?;
                st.pop();
            }
        } else {
            for i in (0..n).rev() {
                let v = st.index_int(src, f + i)?;
                st.push(v);
                let r = st.set_index_int(dst, t + i, v);
                r?;
                st.pop();
            }
        }
    }
    st.push(dst);
    Ok(1)
}

fn tconcat(st: &mut State) -> LuaResult<usize> {
    let tv = st.arg(0);
    let mut last = aux_getn(st, 1, TAB_R)?;
    let sep: &[u8] = match st.opt_str(2)? {
        Some(s) => s.get().as_bytes(),
        None => b"",
    };
    let mut i = st.opt_integer(3, 1)?;
    last = st.opt_integer(4, last)?;
    let mut buf: Vec<u8> = st.take_scratch();
    let addfield = |st: &mut State, i: i64, buf: &mut Vec<u8>| -> LuaResult<()> {
        let v = st.index_int(tv, i)?;
        if let Some(s) = v.as_string() {
            buf.extend_from_slice(s.as_bytes());
        } else if let Some(n) = v.as_number() {
            number::with_tostring(n, |b| buf.extend_from_slice(b));
        } else {
            let mut m = b"invalid value (".to_vec();
            m.extend_from_slice(rn_core::auxlib::typename_of(&v));
            m.extend_from_slice(format!(") at index {i} in table for 'concat'").as_bytes());
            return Err(st.l_error(&m));
        }
        Ok(())
    };
    while i < last {
        addfield(st, i, &mut buf)?;
        buf.extend_from_slice(sep);
        i += 1;
    }
    if i == last {
        addfield(st, i, &mut buf)?;
    }
    st.push_scratch(buf)?;
    Ok(1)
}

/// P-E2: the address of `table.pack` (the J5 recorder inlines it for a fixed argument count).
#[cfg(feature = "jit")]
pub fn jit_pack_addr() -> usize {
    tpack as *const () as usize
}

fn tpack(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let t = st.new_table_sized(n, 1)?;
    st.push(Value::table(t));
    for i in 1..=n {
        let v = st.arg(i - 1);
        st.raw_set_int(t, i as i64, &v)?;
    }
    st.raw_set_str(t, b"n", Value::int(n as i64))?;
    Ok(1)
}

fn tunpack(st: &mut State) -> LuaResult<usize> {
    let tv = st.arg(0);
    #[cfg(not(rustynail_lua54))]
    let len = aux_getn(st, 1, TAB_R)?;
    let mut i = st.opt_integer(2, 1)?;
    #[cfg(not(rustynail_lua54))]
    let e = if st.arg_is_none_or_nil(3) {
        len
    } else {
        st.check_integer(3)?
    };
    #[cfg(rustynail_lua54)]
    let e = if st.arg_is_none_or_nil(3) {
        st.len_int(tv)?
    } else {
        st.check_integer(3)?
    };
    if i > e {
        return Ok(0);
    }
    let mut n = (e as u64).wrapping_sub(i as u64);
    if n >= i32::MAX as u64 || {
        n += 1;
        !can_checkstack(st, n as usize)
    } {
        return Err(st.l_error(b"too many results to unpack"));
    }
    while i < e {
        let v = st.index_int(tv, i)?;
        st.push(v);
        i += 1;
    }
    let v = st.index_int(tv, e)?;
    st.push(v);
    Ok(n as usize)
}

// ---------------------------------------------------------------------------------------------
// quicksort (operates through the stack exactly like the reference)
// ---------------------------------------------------------------------------------------------

const RANLIMIT: u32 = 100;

/// Value `k` slots below the top (`k = 1` is the top).
#[inline]
fn peek(st: &State, k: usize) -> Value {
    st.get(st.top() - k)
}

#[inline]
fn geti(st: &mut State, tv: Value, i: u32) -> LuaResult<()> {
    let v = st.index_int(tv, i as i64)?;
    st.push(v);
    Ok(())
}

#[inline]
fn seti(st: &mut State, tv: Value, i: u32) -> LuaResult<()> {
    let v = peek(st, 1);
    st.set_index_int(tv, i as i64, v)?;
    st.pop();
    Ok(())
}

#[inline]
fn set2(st: &mut State, tv: Value, i: u32, j: u32) -> LuaResult<()> {
    seti(st, tv, i)?;
    seti(st, tv, j)
}

/// `sort_comp(L, -ka, -kb)`: is the value at offset `ka` less than the one at `kb`?
#[inline]
fn sort_comp(st: &mut State, cmp: Value, ka: usize, kb: usize) -> LuaResult<bool> {
    if cmp.is_nil() {
        let (a, b) = (peek(st, ka), peek(st, kb));
        st.compare_lt_le(a, b, false)
    } else {
        let func = st.top();
        let (a, b) = (peek(st, ka), peek(st, kb));
        st.push(cmp);
        st.push(a);
        st.push(b);
        st.call_no_yield(func, 1)?;
        Ok(st.pop().is_truthy())
    }
}

fn partition(st: &mut State, tv: Value, cmp: Value, lo: u32, up: u32) -> LuaResult<u32> {
    let mut i = lo;
    let mut j = up - 1;
    loop {
        loop {
            i = i.wrapping_add(1);
            geti(st, tv, i)?;
            if !sort_comp(st, cmp, 1, 2)? {
                break;
            }
            if i == up - 1 {
                return Err(st.l_error(b"invalid order function for sorting"));
            }
            st.pop();
        }
        loop {
            j = j.wrapping_sub(1);
            geti(st, tv, j)?;
            if !sort_comp(st, cmp, 3, 1)? {
                break;
            }
            if j < i {
                return Err(st.l_error(b"invalid order function for sorting"));
            }
            st.pop();
        }
        if j < i {
            st.pop();
            set2(st, tv, up - 1, i)?;
            return Ok(i);
        }
        set2(st, tv, i, j)?;
    }
}

fn choose_pivot(lo: u32, up: u32, rnd: u32) -> u32 {
    let r4 = (up - lo) / 4;
    #[cfg(rustynail_lua54)]
    let rnd = rnd % (r4 * 2) + (lo + r4);
    #[cfg(not(rustynail_lua54))]
    let rnd = (rnd ^ lo ^ up) % (r4 * 2) + (lo + r4);
    rnd
}

fn auxsort(
    st: &mut State,
    tv: Value,
    cmp: Value,
    mut lo: u32,
    mut up: u32,
    mut rnd: u32,
) -> LuaResult<()> {
    while lo < up {
        geti(st, tv, lo)?;
        geti(st, tv, up)?;
        if sort_comp(st, cmp, 1, 2)? {
            set2(st, tv, lo, up)?;
        } else {
            st.pop();
            st.pop();
        }
        if up - lo == 1 {
            return Ok(());
        }
        let mut p = if up - lo < RANLIMIT || rnd == 0 {
            (lo + up) / 2
        } else {
            choose_pivot(lo, up, rnd)
        };
        geti(st, tv, p)?;
        geti(st, tv, lo)?;
        if sort_comp(st, cmp, 2, 1)? {
            set2(st, tv, p, lo)?;
        } else {
            st.pop();
            geti(st, tv, up)?;
            if sort_comp(st, cmp, 1, 2)? {
                set2(st, tv, p, up)?;
            } else {
                st.pop();
                st.pop();
            }
        }
        if up - lo == 2 {
            return Ok(());
        }
        geti(st, tv, p)?;
        let pv = peek(st, 1);
        st.push(pv);
        geti(st, tv, up - 1)?;
        set2(st, tv, p, up - 1)?;
        p = partition(st, tv, cmp, lo, up)?;
        let n;
        if p - lo < up - p {
            auxsort(st, tv, cmp, lo, p - 1, rnd)?;
            n = p - lo;
            lo = p + 1;
        } else {
            auxsort(st, tv, cmp, p + 1, up, rnd)?;
            n = up - p;
            up = p - 1;
        }
        if up.wrapping_sub(lo) / 128 > n {
            rnd = rn_core::auxlib::make_seed();
        }
    }
    Ok(())
}

fn sort(st: &mut State) -> LuaResult<usize> {
    let tv = st.arg(0);
    let n = aux_getn(st, 1, TAB_RW)?;
    if n > 1 {
        st.arg_check(n < i32::MAX as i64, 1, b"array too big")?;
        if !st.arg_is_none_or_nil(2) {
            st.check_type(2, LuaType::Function)?;
        }
        let cmp = st.arg(1);
        let func = st.ci().r().func;
        st.set_top(func + 1);
        st.push(tv);
        st.push(cmp);
        l_checkstack(st, 8, "sort")?;
        auxsort(st, tv, cmp, 1, n as u32, 0)?;
    }
    Ok(0)
}

/// `luaopen_table`.
pub fn open(st: &mut State) -> LuaResult<usize> {
    let lib = st.new_table_sized(0, 8)?;
    st.push(Value::table(lib));
    st.set_funcs(
        lib,
        &[
            ("concat", Some(tconcat)),
            #[cfg(not(rustynail_lua54))]
            ("create", Some(tcreate)),
            ("insert", Some(tinsert)),
            ("pack", Some(tpack)),
            ("unpack", Some(tunpack)),
            ("remove", Some(tremove)),
            ("move", Some(tmove)),
            ("sort", Some(sort)),
        ],
    )?;
    Ok(1)
}

/// P-B: the addresses of `table.insert` and `table.remove` (the J5 recorder runs the append / pop forms as intrinsics).
#[cfg(feature = "jit")]
pub fn jit_libfns() -> [usize; 2] {
    [tinsert as *const () as usize, tremove as *const () as usize]
}

/// BUNDLE-R: the addresses of `table.unpack` and `table.move`.
#[cfg(feature = "jit")]
pub fn jit_libfns_ext() -> [usize; 2] {
    [tunpack as *const () as usize, tmove as *const () as usize]
}
