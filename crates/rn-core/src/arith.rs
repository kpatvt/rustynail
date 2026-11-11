//! The non-inlined helpers of the interpreter (port of the out-of-line parts of `lvm.c`):
//! `luaV_finishget/finishset`, equality and order comparisons, concatenation, `#`, the numeric
//! `for` preparation and the string -> number coercion points.
//!
//! Everything here is safe code; the hot loop lives in `vm_unsafe.rs`. All functions are
//! `#[inline(never)]`-style slow paths: the interpreter calls them only after its inline fast
//! paths failed.
use crate::error::{OpLoc, Opnd};
use crate::meta::Tm;
use crate::number::{self, Number, F2I};
use crate::state::*;
use crate::status::{LuaResult, Status};
use crate::string::{LuaString, StrRef};
use crate::table::HOK;
use crate::value::*;

/// Limit for `__index`/`__newindex` chains (`MAXTAGLOOP`).
pub const MAXTAGLOOP: usize = 2000;

/// `MAX_SIZE` for strings (`llimits.h`), minus the string header.
const MAX_STRING: usize = (i64::MAX as usize) - 32;

/// String -> number coercion of a value (`l_strton`): `Some` only for strings holding a numeral.
#[inline]
pub fn str_to_number(v: &Value) -> Option<Number> {
    let s = v.as_string()?;
    number::str2num(s.as_bytes())
}

/// `luaV_tonumber_` + the float fast case: any number or numeric string as a float.
#[inline]
pub fn to_float(v: &Value) -> Option<f64> {
    match v.as_number() {
        Some(Number::Float(f)) => Some(f),
        Some(Number::Int(i)) => Some(i as f64),
        None => match str_to_number(v)? {
            Number::Int(i) => Some(i as f64),
            Number::Float(f) => Some(f),
        },
    }
}

/// `luaV_tointegerns`: integers and floats with an exact (or rounded per `mode`) integer value,
/// no string coercion.
#[inline]
pub fn to_integer_ns(v: &Value, mode: F2I) -> Option<i64> {
    match v.as_number()? {
        Number::Int(i) => Some(i),
        Number::Float(f) => number::float_to_int(f, mode),
    }
}

/// `luaV_tointeger`: like [`to_integer_ns`] but numeric strings are accepted.
pub fn to_integer(v: &Value, mode: F2I) -> Option<i64> {
    if let Some(n) = str_to_number(v) {
        return to_integer_ns(&Value::number(n), mode);
    }
    to_integer_ns(v, mode)
}

/// A value of the stack-resident metamethod machinery.
fn tm_is_function(tm: &Value) -> bool {
    tm.is_function()
}

/// B3-4: most operands of a `CONCAT` that [`State::concat_num`] handles.
const CONCAT_NUM_MAX: usize = 8;

impl State {
    /// `luaV_finishget`: complete `res = t[key]` after the raw fast path found nothing (or `t` is
    /// not a table). `t.loc` names the first indexed object for error messages. The result is
    /// stored in stack slot `res`.
    #[cold]
    #[inline(never)]
    pub fn finish_get(&mut self, t: Opnd, key: Value, res: StkId) -> LuaResult<()> {
        let mut obj = t.v;
        let loc = t.loc;
        let mut first = true;
        for _ in 0..MAXTAGLOOP {
            let tm;
            if let Some(h) = obj.as_table() {
                if !first {
                    let v = match key.as_strref() {
                        Some(ks) if key.is_short_string() => h.get_short_str(ks),
                        _ => h.get(&key),
                    };
                    if !v.is_nil() {
                        self.set(res, v);
                        return Ok(());
                    }
                }
                match self.fast_tm(h.metatable(), Tm::Index) {
                    None => {
                        self.set(res, Value::NIL);
                        return Ok(());
                    }
                    Some(m) => tm = m,
                }
            } else {
                tm = self.get_tm_by_obj(&obj, Tm::Index);
                if tm.is_nil() {
                    return Err(self.type_error(
                        Opnd::new(obj, if first { loc } else { OpLoc::None }),
                        "index",
                    ));
                }
            }
            first = false;
            if tm_is_function(&tm) {
                self.call_tm_res(tm, obj, key, res)?;
                return Ok(());
            }
            obj = tm;
        }
        Err(self.run_error(b"'__index' chain too long; possible loop"))
    }

    /// `luaV_finishset`: complete `t[key] = val` after the raw fast path returned `hres != HOK`
    /// (for a table `t`) or `t` is not a table.
    #[inline(never)]
    pub fn finish_set(&mut self, t: Opnd, key: &Value, val: &Value, hres: i32) -> LuaResult<()> {
        let mut obj = t.v;
        let loc = t.loc;
        let mut hres = if obj.is_table() { hres } else { -1 };
        let mut first = true;
        for _ in 0..MAXTAGLOOP {
            let tm;
            if let Some(h) = obj.as_table() {
                debug_assert!(hres != HOK);
                match self.fast_tm(h.metatable(), Tm::NewIndex) {
                    None => {
                        // anchor the table while a (possibly emergency) collection can run
                        self.check_stack(1)?;
                        self.push(obj);
                        let r = if first {
                            h.finish_set(self.heap(), key, val, hres)
                        } else {
                            h.set(self.heap(), key, val)
                        };
                        self.pop();
                        if let Err(e) = r {
                            return Err(self.table_error(e));
                        }
                        h.invalidate_tm_cache();
                        self.heap().barrier_back(h.as_ptr().cast(), val);
                        return Ok(());
                    }
                    Some(m) => tm = m,
                }
            } else {
                tm = self.get_tm_by_obj(&obj, Tm::NewIndex);
                if tm.is_nil() {
                    return Err(self.type_error(
                        Opnd::new(obj, if first { loc } else { OpLoc::None }),
                        "index",
                    ));
                }
            }
            first = false;
            if tm_is_function(&tm) {
                return self.call_tm(tm, obj, *key, *val);
            }
            obj = tm;
            // equivalent of `luaV_fastset` on the metamethod object
            if let Some(h) = obj.as_table() {
                hres = h.pset(*key, *val);
                if hres == HOK {
                    self.heap().barrier_back(h.as_ptr().cast(), val);
                    return Ok(());
                }
            }
        }
        let _ = hres;
        Err(self.run_error(b"'__newindex' chain too long; possible loop"))
    }

    /// `luaV_equalobj` (with `__eq`).
    #[inline(never)]
    pub fn equal_obj(&mut self, a: Value, b: Value) -> LuaResult<bool> {
        if a.type_tag() != b.type_tag() {
            return Ok(false);
        }
        if a.raw_equal(&b) {
            return Ok(true);
        }
        if a.tag() != b.tag() {
            return Ok(false);
        }
        let tm = if let (Some(x), Some(y)) = (a.as_table(), b.as_table()) {
            let m = self.fast_tm(x.metatable(), Tm::Eq);
            m.or_else(|| self.fast_tm(y.metatable(), Tm::Eq))
        } else if let (Some(x), Some(y)) = (a.as_userdata(), b.as_userdata()) {
            let m = x.metatable().and_then(|mt| self.fast_tm(Some(mt), Tm::Eq));
            m.or_else(|| y.metatable().and_then(|mt| self.fast_tm(Some(mt), Tm::Eq)))
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

    /// `luaV_lessthan`.
    #[inline(never)]
    #[cold]
    pub fn less_than(&mut self, a: Value, b: Value) -> LuaResult<bool> {
        if let (Some(x), Some(y)) = (a.as_number(), b.as_number()) {
            return Ok(number::lt_num(x, y));
        }
        if let (Some(x), Some(y)) = (a.as_string(), b.as_string()) {
            return Ok(x.as_bytes() < y.as_bytes());
        }
        self.call_order_tm(a, b, Tm::Lt)
    }

    /// `luaV_lessequal`.
    #[inline(never)]
    #[cold]
    pub fn less_equal(&mut self, a: Value, b: Value) -> LuaResult<bool> {
        if let (Some(x), Some(y)) = (a.as_number(), b.as_number()) {
            return Ok(number::le_num(x, y));
        }
        if let (Some(x), Some(y)) = (a.as_string(), b.as_string()) {
            return Ok(x.as_bytes() <= y.as_bytes());
        }
        self.call_order_tm(a, b, Tm::Le)
    }

    /// `tostring` macro of `luaV_concat`: make the value in `slot` a string (numbers are converted
    /// in place). `false` if it is neither.
    fn concat_tostring(&mut self, slot: StkId) -> LuaResult<bool> {
        let v = self.get(slot);
        if v.is_string() {
            return Ok(true);
        }
        if let Some(n) = v.as_number() {
            let s = number::with_tostring(n, |b| self.new_str_fresh(b).map(Value::string))?;
            self.set(slot, s);
            return Ok(true);
        }
        Ok(false)
    }

    /// `luaV_concat`: concatenate the `total` values on top of the stack, leaving the result in
    /// `top - 1` (the stack shrinks by `total - 1`).
    #[inline(never)]
    pub fn concat(&mut self, total: usize) -> LuaResult<()> {
        if total == 2 {
            // `s .. ""` / `"" .. s`: the result is the other operand (no conversion, no
            // allocation). Kept in this small entry function: the full scan below needs a large
            // stack frame and all callee-saved registers.
            let first = self.top() - 2;
            let (v1, v2) = (self.get(first), self.get(first + 1));
            // (tag tests first: `as_strref` alone also tests the pointer for null)
            if v1.is_string() && v2.is_string() {
                let empty = |v: &Value| v.as_strref().is_some_and(|s| s.get().is_empty());
                if empty(&v2) {
                    self.set_top(first + 1);
                    return Ok(());
                }
                if empty(&v1) {
                    self.set(first, v2);
                    self.set_top(first + 1);
                    return Ok(());
                }
            }
        } else if total == 1 {
            return Ok(());
        }
        self.concat_scan(total)
    }

    /// `concat` for operands other than two strings with an empty one.
    #[inline(never)]
    fn concat_scan(&mut self, total: usize) -> LuaResult<()> {
        // Fast path: all operands are non-empty strings or numbers (the reference's loop then
        // makes one pass over all of them, converting numbers in place from the top down, as
        // here; with an empty operand it returns the other operand unchanged, which the general
        // path below reproduces, as it does metamethods).
        let first = self.top() - total;
        let mut tl = 0usize;
        let mut j = total;
        let mut tried_num = total > CONCAT_NUM_MAX;
        while j > 0 {
            let slot = first + (j - 1);
            let mut v = self.get(slot);
            if !v.is_string() {
                if !v.is_number() {
                    break;
                }
                if !tried_num {
                    // B3-4: numbers among the operands: build the result without interning the number strings
                    tried_num = true;
                    if self.concat_num(total)? {
                        return Ok(());
                    }
                }
                // the reference converts the top operand only if the one below it can be
                // concatenated too (otherwise `__concat` receives the number)
                if j == total {
                    let below = self.get(slot - 1);
                    if !(below.is_string() || below.is_number()) {
                        break;
                    }
                }
                self.concat_tostring(slot)?;
                v = self.get(slot);
            }
            // SAFETY: `v` is a string (checked or converted just above).
            let l = unsafe { v.as_strref().unwrap_unchecked() }.get().len();
            if l == 0 || l >= MAX_STRING - tl {
                break;
            }
            tl += l;
            j -= 1;
        }
        if j == 0 {
            let res = self.concat_result(first, total, tl)?;
            self.set(first, Value::string(res));
            self.set_top(first + 1);
            return Ok(());
        }
        self.concat_general(total)
    }

    /// B3-4: `concat_scan` for at most [`CONCAT_NUM_MAX`] operands that are all non-empty strings or numbers: the numbers
    /// are formatted into stack buffers and the result is made straight from the pieces (no intermediate interned
    /// string per number: one string-table lookup instead of one per number plus one). The converted operands are not
    /// stored back (the operand registers are dead temporaries after the concatenation). `Ok(false)`, with nothing
    /// changed, when an operand is empty / not a string or number / too long: the caller's general path handles it.
    #[inline(never)]
    fn concat_num(&mut self, total: usize) -> LuaResult<bool> {
        let first = self.top() - total;
        let mut texts = [[0u8; 32]; CONCAT_NUM_MAX];
        let mut parts: [(*const u8, usize); CONCAT_NUM_MAX] = [(std::ptr::null(), 0); CONCAT_NUM_MAX];
        let mut tl = 0usize;
        for j in 0..total {
            let v = self.get(first + j);
            let (p, l) = if v.is_string() {
                // SAFETY: tag tested.
                let b = unsafe { v.as_strref().unwrap_unchecked() }.get().as_bytes();
                (b.as_ptr(), b.len())
            } else if let Some(n) = v.as_number() {
                let t = &mut texts[j];
                let l = number::with_tostring(n, |b| {
                    t[..b.len()].copy_from_slice(b);
                    b.len()
                });
                (t.as_ptr(), l)
            } else {
                return Ok(false);
            };
            if l == 0 || l >= MAX_STRING - tl {
                return Ok(false);
            }
            parts[j] = (p, l);
            tl += l;
        }
        let res = if tl <= crate::string::MAX_SHORT_LEN {
            let mut buf = [0u8; crate::string::MAX_SHORT_LEN];
            let mut off = 0;
            for &(p, l) in &parts[..total] {
                // SAFETY: `p` points to `l` live bytes (a string anchored on the stack, or `texts`).
                buf[off..off + l].copy_from_slice(unsafe { std::slice::from_raw_parts(p, l) });
                off += l;
            }
            self.new_str_fresh(&buf[..tl])?
        } else {
            let attempt = |st: &mut State| {
                // SAFETY: as above; the operand strings stay anchored on the stack across an emergency collection.
                unsafe {
                    st.heap().new_long_str_with(tl, |dst| {
                        let mut off = 0;
                        for &(p, l) in &parts[..total] {
                            std::ptr::copy_nonoverlapping(p, dst.add(off), l);
                            off += l;
                        }
                    })
                }
            };
            match attempt(self) {
                Err(Status::ErrMem) if self.can_emergency_gc() => {
                    self.collect(true);
                    attempt(self)?
                }
                r => r?,
            }
        };
        self.set(first, Value::string(res));
        self.set_top(first + 1);
        Ok(true)
    }

    /// `luaV_concat` for operands that need conversions, metamethods or have empty strings.
    #[inline(never)]
    fn concat_general(&mut self, mut total: usize) -> LuaResult<()> {
        if total == 2 {
            // two strings, one of them empty? (`s .. ""`, `"" .. s`)
            let top = self.top();
            let (v2, v1) = (self.get(top - 2), self.get(top - 1));
            if v1.is_string() && v2.is_string() {
                let empty = |v: &Value| v.as_string().is_some_and(|s| s.is_empty());
                if empty(&v1) {
                    self.set_top(top - 1); // result is the first operand
                    return Ok(());
                } else if empty(&v2) {
                    self.set(top - 2, v1); // result is the second operand
                    self.set_top(top - 1);
                    return Ok(());
                }
            }
        }
        // a string operand, or a number converted in place (`tostring`)
        macro_rules! tostr {
            ($slot:expr) => {{
                let slot = $slot;
                self.get(slot).is_string() || self.concat_tostring(slot)?
            }};
        }
        let is_empty =
            |v: &Value| v.is_short_string() && v.as_string().is_some_and(|s| s.is_empty());
        loop {
            let top = self.top();
            let mut n = 2usize;
            let v2 = self.get(top - 2);
            if !(v2.is_string() || v2.is_number()) || !tostr!(top - 1) {
                self.try_concat_tm()?;
            } else if is_empty(&self.get(top - 1)) {
                let _ = tostr!(top - 2); // result is the first operand (as a string)
            } else if is_empty(&v2) {
                let v1 = self.get(top - 1);
                self.set(top - 2, v1); // result is the second operand
            } else {
                // at least two string values; get as many as possible
                let mut tl = self.get(top - 1).as_string().map_or(0, |s| s.len());
                n = 1;
                while n < total && tostr!(top - n - 1) {
                    let l = self.get(top - n - 1).as_string().map_or(0, |s| s.len());
                    if l >= MAX_STRING - tl {
                        self.set_top(top - total); // pop strings to avoid wasting stack
                        return Err(self.run_error(b"string length overflow"));
                    }
                    tl += l;
                    n += 1;
                }
                let res = self.concat_result(top - n, n, tl)?;
                self.set(top - n, Value::string(res));
            }
            total -= n - 1;
            let t = self.top();
            self.set_top(t - (n - 1));
            if total <= 1 {
                break;
            }
        }
        Ok(())
    }

    /// The string made of the `n` strings at `first..` (`tl` bytes), built without an intermediate
    /// buffer: short results are hashed/compared/copied piecewise, long ones written in place.
    #[inline(always)]
    fn concat_result(&mut self, first: StkId, n: usize, tl: usize) -> LuaResult<StrRef> {
        let attempt = |st: &mut State| {
            let parts = st.slot_ptr(first).cast_const();
            // SAFETY: the `n` slots from `first` hold strings with `tl` bytes in total, anchored
            // on the stack; no collection runs inside these calls.
            unsafe {
                if tl <= crate::string::MAX_SHORT_LEN {
                    st.heap().intern_concat(parts, n, tl)
                } else {
                    st.heap().new_long_str_with(tl, |dst| {
                        let mut off = 0;
                        for j in 0..n {
                            let s = Value::load(parts.add(j)).as_strref().unwrap_unchecked();
                            let b = LuaString::bytes_raw(s.as_ptr());
                            std::ptr::copy_nonoverlapping(b.as_ptr(), dst.add(off), b.len());
                            off += b.len();
                        }
                    })
                }
            }
        };
        match attempt(self) {
            Err(Status::ErrMem) if self.can_emergency_gc() => {
                self.collect(true); // (may move the stack: `attempt` re-derives it)
                attempt(self)
            }
            r => r,
        }
    }

    /// `luaV_objlen`: `ra = #rb`.
    #[inline(never)]
    #[cold]
    pub fn obj_len(&mut self, ra: StkId, rb: Opnd) -> LuaResult<()> {
        let v = rb.v;
        let tm;
        if let Some(h) = v.as_table() {
            match self.fast_tm(h.metatable(), Tm::Len) {
                Some(m) => tm = m,
                None => {
                    let n = h.len(self.heap());
                    self.set(ra, Value::int(n as i64));
                    return Ok(());
                }
            }
        } else if let Some(s) = v.as_string() {
            let n = s.len();
            self.set(ra, Value::int(n as i64));
            return Ok(());
        } else {
            tm = self.get_tm_by_obj(&v, Tm::Len);
            if tm.is_nil() {
                return Err(self.type_error(rb, "get length of"));
            }
        }
        self.call_tm_res(tm, v, v, ra)?;
        Ok(())
    }

    /// `forlimit`: returns `Ok(Some(limit))`, or `Ok(None)` if the loop must not run.
    fn for_limit(&mut self, init: i64, lim: &Value, step: i64) -> LuaResult<Option<i64>> {
        let mode = if step < 0 { F2I::Ceil } else { F2I::Floor };
        let limit = match to_integer(lim, mode) {
            Some(l) => l,
            None => {
                // not coercible to an integer: try to convert to float
                let Some(flim) = to_float(lim) else {
                    return Err(self.for_error(lim, "limit"));
                };
                // else `flim` is a float out of integer bounds
                if 0.0 < flim {
                    if step < 0 {
                        return Ok(None);
                    }
                    i64::MAX
                } else {
                    if step > 0 {
                        return Ok(None);
                    }
                    i64::MIN
                }
            }
        };
        let skip = if step > 0 { init > limit } else { init < limit };
        Ok(if skip { None } else { Some(limit) })
    }

    /// `forprep`: returns true if the loop must be skipped.
    #[inline(never)]
    pub fn for_prep(&mut self, ra: StkId) -> LuaResult<bool> {
        #[cfg(rustynail_lua54)]
        if let Some(skip) = crate::vm_unsafe::forprep_int54(self, ra) {
            return Ok(skip);
        }
        let (pinit, plimit, pstep) = (self.get(ra), self.get(ra + 1), self.get(ra + 2));
        if let (Some(init), Some(step)) = (pinit.as_int(), pstep.as_int()) {
            if step == 0 {
                return Err(self.run_error(b"'for' step is zero"));
            }
            #[cfg(rustynail_lua54)]
            self.set(ra + 3, Value::int(init)); // control variable
            let Some(limit) = self.for_limit(init, &plimit, step)? else {
                return Ok(true);
            };
            let count = if step > 0 {
                let c = (limit as u64).wrapping_sub(init as u64);
                if step != 1 {
                    c / step as u64
                } else {
                    c
                }
            } else {
                let c = (init as u64).wrapping_sub(limit as u64);
                c / ((step.wrapping_add(1)).wrapping_neg() as u64).wrapping_add(1)
            };
            #[cfg(not(rustynail_lua54))]
            {
                self.set(ra, Value::int(count as i64));
                self.set(ra + 1, Value::int(step));
                self.set(ra + 2, Value::int(init));
            }
            // 5.4: `ra` internal index, `ra + 1` counter (in place of the limit), `ra + 2` step,
            // `ra + 3` control variable (already set above)
            #[cfg(rustynail_lua54)]
            self.set(ra + 1, Value::int(count as i64));
            Ok(false)
        } else {
            let Some(limit) = to_float(&plimit) else {
                return Err(self.for_error(&plimit, "limit"));
            };
            let Some(step) = to_float(&pstep) else {
                return Err(self.for_error(&pstep, "step"));
            };
            let Some(init) = to_float(&pinit) else {
                return Err(self.for_error(&pinit, "initial value"));
            };
            if step == 0.0 {
                return Err(self.run_error(b"'for' step is zero"));
            }
            if if 0.0 < step {
                limit < init
            } else {
                init < limit
            } {
                return Ok(true);
            }
            #[cfg(not(rustynail_lua54))]
            {
                self.set(ra, Value::float(limit));
                self.set(ra + 1, Value::float(step));
                self.set(ra + 2, Value::float(init));
            }
            #[cfg(rustynail_lua54)]
            {
                self.set(ra + 1, Value::float(limit));
                self.set(ra + 2, Value::float(step));
                self.set(ra, Value::float(init)); // internal index
                self.set(ra + 3, Value::float(init)); // control variable
            }
            Ok(false)
        }
    }
}
