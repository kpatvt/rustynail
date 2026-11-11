//! Metamethod events (`ltm.h`): enumeration order, names, and the "absence cache" flag bits kept in
//! `Table::flags`.

/// `TMS` — **order matters** (the first six have cache bits in table flags).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tm {
    Index = 0,
    NewIndex,
    Gc,
    Mode,
    Len,
    /// last tag method with fast access
    Eq,
    Add,
    Sub,
    Mul,
    Mod,
    Pow,
    Div,
    IDiv,
    BAnd,
    BOr,
    BXor,
    Shl,
    Shr,
    Unm,
    BNot,
    Lt,
    Le,
    Concat,
    Call,
    Close,
}

impl Tm {
    /// The event with number `n` (clamped to the last event; used to decode `MMBIN*` operands).
    #[inline]
    pub fn from_u8(n: u8) -> Tm {
        const ALL: [Tm; TM_N] = [
            Tm::Index,
            Tm::NewIndex,
            Tm::Gc,
            Tm::Mode,
            Tm::Len,
            Tm::Eq,
            Tm::Add,
            Tm::Sub,
            Tm::Mul,
            Tm::Mod,
            Tm::Pow,
            Tm::Div,
            Tm::IDiv,
            Tm::BAnd,
            Tm::BOr,
            Tm::BXor,
            Tm::Shl,
            Tm::Shr,
            Tm::Unm,
            Tm::BNot,
            Tm::Lt,
            Tm::Le,
            Tm::Concat,
            Tm::Call,
            Tm::Close,
        ];
        ALL[(n as usize).min(TM_N - 1)]
    }
}

/// `TM_N`.
pub const TM_N: usize = 25;

/// Event names, in `Tm` order.
pub const TM_NAMES: [&str; TM_N] = [
    "__index",
    "__newindex",
    "__gc",
    "__mode",
    "__len",
    "__eq",
    "__add",
    "__sub",
    "__mul",
    "__mod",
    "__pow",
    "__div",
    "__idiv",
    "__band",
    "__bor",
    "__bxor",
    "__shl",
    "__shr",
    "__unm",
    "__bnot",
    "__lt",
    "__le",
    "__concat",
    "__call",
    "__close",
];

/// `maskflags`: a 1 bit means "metamethod known absent".
pub const MASKFLAGS: u8 = (1u8 << (Tm::Eq as u8 + 1)) - 1;

// ---------------------------------------------------------------------------------------------
// ltm.c: metamethod lookup and calls
// ---------------------------------------------------------------------------------------------
use crate::error::Opnd;
use crate::func::Proto;
use crate::state::{Ci, State, StkId, LUA_MULTRET};
use crate::status::LuaResult;
use crate::table::TableRef;
use crate::value::*;

/// `luaT_typenames_` (indexed by `type_tag`, with "no value" at -1 handled by callers).
const TYPE_NAMES: [&[u8]; 12] = [
    b"nil",
    b"boolean",
    b"userdata",
    b"number",
    b"string",
    b"table",
    b"function",
    b"userdata",
    b"thread",
    b"upvalue",
    b"proto",
    b"nil",
];

/// Basic type name (`ttypename`).
pub fn type_name(tag: u8) -> &'static [u8] {
    TYPE_NAMES.get(tag as usize).copied().unwrap_or(b"?")
}

impl State {
    /// Metatable of any value (`G(L)->mt[ttype]` for non-table/userdata).
    pub fn metatable_of(&self, o: &Value) -> Option<TableRef> {
        match o.type_tag() {
            LUA_TTABLE => o.as_table().and_then(|t| t.metatable()),
            LUA_TUSERDATA => o.as_userdata().and_then(|u| u.metatable()),
            t => self.g().mt.get(t as usize).copied().flatten(),
        }
    }

    /// `luaT_gettm` for a fast event (flag-cached absence).
    pub fn fast_tm(&mut self, mt: Option<TableRef>, e: Tm) -> Option<Value> {
        self.g().heap.fast_tm(mt, e)
    }

    /// `luaT_gettmbyobj`: the metamethod `ev` of `o`, or nil.
    pub fn get_tm_by_obj(&self, o: &Value, ev: Tm) -> Value {
        match self.metatable_of(o) {
            None => Value::NIL,
            Some(mt) => {
                let name = self.g().heap.tm_name(ev);
                mt.get_short_str(name)
            }
        }
    }

    /// `luaT_objtypename`: type name honouring a string `__name` in the metatable.
    pub fn obj_typename<'a>(&'a self, o: &Value) -> &'a [u8] {
        if o.is_table() || o.is_userdata() {
            if let Some(mt) = self.metatable_of(o) {
                let name = mt.get_short_str(self.g().name_key);
                if let Some(s) = name.as_strref() {
                    return s.get().as_bytes();
                }
            }
        }
        type_name(o.type_tag())
    }

    /// `luaT_callTM`: call `f(p1, p2, p3)` discarding results. A metamethod may yield only when
    /// called from Lua code.
    pub fn call_tm(&mut self, f: Value, p1: Value, p2: Value, p3: Value) -> LuaResult<()> {
        let func = self.top();
        self.set(func, f);
        self.set(func + 1, p1);
        self.set(func + 2, p2);
        self.set(func + 3, p3);
        self.set_top(func + 4);
        if self.ci().r().is_lua_code() {
            self.call(func, 0)
        } else {
            self.call_no_yield(func, 0)
        }
    }

    /// `luaT_callTMres`: call `f(p1, p2)` and store the single result in `res`; also returns it.
    pub fn call_tm_res(&mut self, f: Value, p1: Value, p2: Value, res: StkId) -> LuaResult<Value> {
        let func = self.top();
        self.set(func, f);
        self.set(func + 1, p1);
        self.set(func + 2, p2);
        self.set_top(func + 3);
        if self.ci().r().is_lua_code() {
            self.call(func, 1)?;
        } else {
            self.call_no_yield(func, 1)?;
        }
        let v = self.pop();
        self.set(res, v);
        Ok(v)
    }

    /// `callbinTM`: try the first operand's metamethod, then the second's.
    fn call_bin_tm(
        &mut self,
        p1: Value,
        p2: Value,
        res: StkId,
        ev: Tm,
    ) -> LuaResult<Option<Value>> {
        let mut tm = self.get_tm_by_obj(&p1, ev);
        if tm.is_nil() {
            tm = self.get_tm_by_obj(&p2, ev);
        }
        if tm.is_nil() {
            return Ok(None);
        }
        self.call_tm_res(tm, p1, p2, res).map(Some)
    }

    /// `luaT_trybinTM`: arithmetic/bitwise metamethod with the reference's error messages.
    #[cold]
    pub fn try_bin_tm(&mut self, p1: Opnd, p2: Opnd, res: StkId, ev: Tm) -> LuaResult<()> {
        if self.call_bin_tm(p1.v, p2.v, res, ev)?.is_some() {
            return Ok(());
        }
        Err(match ev {
            Tm::BAnd | Tm::BOr | Tm::BXor | Tm::Shl | Tm::Shr | Tm::BNot => {
                if p1.v.is_number() && p2.v.is_number() {
                    self.toint_error(p1, p2)
                } else {
                    self.opint_error(p1, p2, "perform bitwise operation on")
                }
            }
            _ => self.opint_error(p1, p2, "perform arithmetic on"),
        })
    }

    /// `luaT_tryconcatTM`: operands are the two values on top of the stack; the result replaces them.
    #[cold]
    pub fn try_concat_tm(&mut self) -> LuaResult<()> {
        let p1 = self.top() - 2;
        let (a, b) = (self.get(p1), self.get(p1 + 1));
        if self.call_bin_tm(a, b, p1, Tm::Concat)?.is_none() {
            let (l1, l2) = (
                Opnd::new(a, crate::error::OpLoc::Stack(p1)),
                Opnd::new(b, crate::error::OpLoc::Stack(p1 + 1)),
            );
            return Err(self.concat_error(l1, l2));
        }
        Ok(())
    }

    /// `luaT_trybinassocTM`.
    #[cold]
    pub fn try_bin_assoc_tm(
        &mut self,
        p1: Opnd,
        p2: Opnd,
        flip: bool,
        res: StkId,
        ev: Tm,
    ) -> LuaResult<()> {
        if flip {
            self.try_bin_tm(p2, p1, res, ev)
        } else {
            self.try_bin_tm(p1, p2, res, ev)
        }
    }

    /// `luaT_trybiniTM`: second operand is an immediate integer.
    #[cold]
    pub fn try_bini_tm(
        &mut self,
        p1: Opnd,
        i2: i64,
        flip: bool,
        res: StkId,
        ev: Tm,
    ) -> LuaResult<()> {
        self.try_bin_assoc_tm(p1, Opnd::from(Value::int(i2)), flip, res, ev)
    }

    /// `luaT_callorderTM`: order comparison through `__lt`/`__le`.
    pub fn call_order_tm(&mut self, p1: Value, p2: Value, ev: Tm) -> LuaResult<bool> {
        let res = self.top();
        match self.call_bin_tm(p1, p2, res, ev)? {
            Some(v) => Ok(v.is_truthy()),
            #[cfg(rustynail_lua54)]
            None if ev == Tm::Le => {
                // LUA_COMPAT_LT_LE (the 5.4 reference is built with LUA_COMPAT_5_3): try
                // `!(p2 < p1)` for `p1 <= p2`
                let ci = self.ci();
                ci.w().callstatus |= crate::state::CIST_LEQ; // mark it is doing 'lt' for 'le'
                let res = self.top();
                match self.call_bin_tm(p2, p1, res, Tm::Lt)? {
                    Some(v) => {
                        ci.w().callstatus ^= crate::state::CIST_LEQ; // clear mark
                        Ok(v.is_falsy())
                    }
                    None => Err(self.order_error(&p1, &p2)),
                }
            }
            None => Err(self.order_error(&p1, &p2)),
        }
    }

    /// `luaT_callorderiTM`: comparison against an immediate (`flip` when operands were swapped).
    #[cold]
    pub fn call_orderi_tm(
        &mut self,
        p1: Value,
        v2: i32,
        flip: bool,
        isfloat: bool,
        ev: Tm,
    ) -> LuaResult<bool> {
        let aux = if isfloat {
            Value::float(v2 as f64)
        } else {
            Value::int(v2 as i64)
        };
        if flip {
            self.call_order_tm(aux, p1, ev)
        } else {
            self.call_order_tm(p1, aux, ev)
        }
    }

    // ---- varargs ------------------------------------------------------------------------------

    /// `createvarargtab`: new table with the `n` values starting at `f`, field `n`; pushed on top.
    #[cfg(not(rustynail_lua54))]
    fn create_vararg_tab(&mut self, f: StkId, n: usize) -> LuaResult<()> {
        let t = self.new_table()?;
        self.check_stack(1)?;
        self.push(Value::table(t));
        t.resize(self.heap(), n as u32, 1)
            .map_err(|e| self.table_error(e))?;
        let key = self.new_str_value(b"n")?;
        // no GC can happen before the next safepoint: the resize made room
        self.raw_set(t, &key, &Value::int(n as i64))?;
        for i in 0..n {
            let v = self.get(f + i);
            self.raw_set_int(t, i as i64 + 1, &v)?;
        }
        self.check_gc();
        Ok(())
    }

    /// `luaT_adjustvarargs` (called by `OP_VARARGPREP`): lay out the frame of a vararg function.
    pub fn adjust_varargs(&mut self, ci: Ci, p: Gc<Proto>) -> LuaResult<()> {
        let func = ci.r().func;
        let totalargs = self.top() - func - 1;
        let nfixparams = p.numparams as usize;
        let nextra = totalargs - nfixparams;
        #[cfg(not(rustynail_lua54))]
        if p.flag & crate::protodata::PF_VATAB != 0 {
            debug_assert!(p.flag & crate::protodata::PF_VAHID == 0);
            self.create_vararg_tab(func + nfixparams + 1, nextra)?;
            // move table to its proper place (last parameter)
            let t = self.get(self.top() - 1);
            self.set(func + nfixparams + 1, t);
            return Ok(());
        }
        // the copies below, with raw slot pointers (in the interpreter's unsafe module)
        if nfixparams <= totalargs {
            return crate::vm_unsafe::adjust_varargs_fast(self, ci, p);
        }
        // build hidden args:
        // initial stack:  func arg1 ... argn extra1 ...      final: func nil ... nil extra1 ... func arg1 ... argn
        ci.w().nextraargs = nextra as i32;
        self.check_stack(p.maxstacksize as usize + 1)?;
        // copy function to the top of the stack, after extra arguments
        let mut top = self.top();
        let f = self.get(func);
        self.set(top, f);
        top += 1;
        // move fixed parameters to after the copied function
        for i in 1..=nfixparams {
            let v = self.get(func + i);
            self.set(top, v);
            top += 1;
            self.set(func + i, Value::NIL); // erase original parameter (for GC)
        }
        self.set_top(top);
        ci.w().func += totalargs + 1;
        ci.w().top += totalargs + 1;
        // set the vararg parameter slot to nil
        let nf = ci.r().func + nfixparams + 1;
        self.set(nf, Value::NIL);
        debug_assert!(self.top() <= ci.r().top && ci.r().top.0 <= self.stack_size());
        Ok(())
    }

    /// `luaT_getvararg`: `ra = ...[rc]` / `...n` for a function without a vararg table.
    pub fn get_vararg(&mut self, ci: Ci, ra: StkId, rc: &Value) {
        let nextra = ci.r().nextraargs as i64;
        // `luaV_tointegerns`: numbers only; the string "1" is not a valid index
        let key = match (rc.as_int(), rc.as_float()) {
            (Some(i), _) => Some(i),
            (None, Some(f)) => crate::number::float_to_int(f, crate::number::F2I::Eq),
            _ => None,
        };
        if let Some(n) = key {
            if (n as u64).wrapping_sub(1) < nextra as u64 {
                let slot = ci.r().func - nextra as usize + n as usize - 1;
                let v = self.get(slot);
                self.set(ra, v);
                return;
            }
        } else if let Some(s) = rc.as_string() {
            if s.as_bytes() == b"n" {
                self.set(ra, Value::int(nextra));
                return;
            }
        }
        self.set(ra, Value::NIL);
    }

    #[cfg(not(rustynail_lua54))]
    fn get_num_args(&mut self, ci: Ci, h: Option<TableRef>) -> LuaResult<usize> {
        match h {
            None => Ok(ci.r().nextraargs as usize),
            Some(h) => {
                let key = self.new_str(b"n")?;
                let v = h.get_short_str(key);
                match v.as_int() {
                    Some(n) if (n as u64) <= (i32::MAX / 2) as u64 => Ok(n as usize),
                    _ => Err(self.run_error(b"vararg table has no proper 'n'")),
                }
            }
        }
    }

    /// `luaT_getvarargs`: copy `wanted` (or all if `LUA_MULTRET`-like negative) vararg values to
    /// `where_`. `vatab` is the register of the vararg table, or -1 when there is none.
    pub fn get_varargs(&mut self, ci: Ci, where_: StkId, wanted: i32, vatab: i32) -> LuaResult<()> {
        if vatab < 0 {
            // values from the hidden extra arguments: plain slot copies
            let n = if wanted >= 0 {
                wanted as usize
            } else {
                let n = ci.r().nextraargs as usize;
                // (`checkstackp`: relative to the current top; offsets stay valid)
                self.check_stack(n)?;
                self.set_top(where_ + n);
                n
            };
            crate::vm_unsafe::varargs_fixed(self, ci, where_, n);
            return Ok(());
        }
        #[cfg(not(rustynail_lua54))]
        let h = if vatab < 0 {
            None
        } else {
            self.get(ci.r().func + vatab as usize + 1).as_table()
        };
        #[cfg(rustynail_lua54)]
        let h: Option<TableRef> = {
            let _ = vatab;
            None
        };
        #[cfg(not(rustynail_lua54))]
        let nargs = self.get_num_args(ci, h)?;
        #[cfg(rustynail_lua54)]
        let nargs = ci.r().nextraargs as usize;
        let wanted_n: usize;
        let touse: usize;
        if wanted < 0 {
            wanted_n = nargs;
            touse = nargs;
            // ensure stack space (`checkstackp`: relative to the current top; offsets stay valid)
            self.check_stack(nargs)?;
            self.set_top(where_ + nargs);
        } else {
            wanted_n = wanted as usize;
            touse = nargs.min(wanted_n);
        }
        let mut i = 0;
        match h {
            None => {
                while i < touse {
                    let v = self.get(ci.r().func - nargs + i);
                    self.set(where_ + i, v);
                    i += 1;
                }
            }
            Some(h) => {
                while i < touse {
                    let v = h.get_int(i as i64 + 1);
                    self.set(where_ + i, if v.is_nil() { Value::NIL } else { v });
                    i += 1;
                }
            }
        }
        while i < wanted_n {
            self.set(where_ + i, Value::NIL);
            i += 1;
        }
        let _ = LUA_MULTRET;
        Ok(())
    }
}
