//! Prototypes, closures and upvalues (port of `lfunc.c`, plus the object layouts of `lobject.h`).
//!
//! * [`Proto`] is built from the compiler's plain [`ProtoData`] by [`Proto::from_data`]; string
//!   constants/names are *referenced* (the `StrRef`s are the heap's own objects), not copied.
//! * [`LClosure`] / [`CClosure`] carry their upvalues in a trailing array (flexible-array layout;
//!   accessed only through the full-provenance raw pointer of the owning [`Gc`] handle).
//! * [`UpVal`] is open (`v` points into a thread's stack, linked into that thread's `openupval`
//!   list) or closed (`v` points at the upvalue's own `u.value`).
//! * To-be-closed variables are tracked in a per-thread [`TbcList`] (a fallible stack of indices)
//!   instead of the reference's delta chain stored inside stack slots.
//!
//! `unsafe` here: trailing arrays, the intrusive open-upvalue list, raw object creation.
//! **Invariants:** (F1) `LClosure.upvals`/`CClosure.upvalue` have exactly `nupvalues` initialised
//! slots (upvalue pointers may be null only while the closure is being built, before the next GC
//! safepoint); (F2) an open `UpVal.v` points into the owning thread's stack buffer and the thread's
//! `openupval` list is sorted by decreasing level, linked through `u.open.{next,previous}`;
//! (F3) closed `UpVal.v == &u.value`; (F4) `Proto` arrays are `RawBuf`s whose capacity is the logical
//! length and whose slots are initialised (pointer arrays are NULL-filled first while building).
use crate::alloc::{Mem, RawBuf};
use crate::gc::GcHeader;
use crate::heap::Heap;
use crate::meta::Tm;
use crate::protodata::{Const, ProtoData};
use crate::state::*;
use crate::status::{LuaResult, Status};
use crate::string::{LuaString, StrRef};
use crate::value::*;

/// `MAXUPVAL`.
pub const MAXUPVAL: usize = 255;
/// Special status asking `close` to keep the top of the stack (`CLOSEKTOP`).
pub const CLOSEKTOP: u8 = 6;

// ---------------------------------------------------------------------------------------------
// Proto
// ---------------------------------------------------------------------------------------------

pub use crate::protodata::AbsLineInfo;

/// Upvalue description (`Upvaldesc`).
#[derive(Clone, Copy, Debug)]
pub struct UpvalDesc {
    /// Name (may be null when stripped).
    pub(crate) name: *mut LuaString,
    pub instack: bool,
    pub idx: u8,
    pub kind: u8,
}

impl UpvalDesc {
    pub fn name(&self) -> Option<StrRef> {
        // a non-null name is a live string (F4)
        std::ptr::NonNull::new(self.name).map(StrRef)
    }
}

/// Local variable info (`LocVar`).
#[derive(Clone, Copy, Debug)]
pub struct LocVar {
    pub(crate) varname: *mut LuaString,
    pub startpc: u32,
    pub endpc: u32,
}

impl LocVar {
    pub fn name(&self) -> StrRef {
        // SAFETY: names of local variables are live strings (F4), never null.
        StrRef(unsafe { std::ptr::NonNull::new_unchecked(self.varname) })
    }
}

/// The function prototype GC object.
#[repr(C)]
pub struct Proto {
    pub(crate) hdr: GcHeader,
    pub numparams: u8,
    /// `PF_VAHID` | `PF_VATAB` (`protodata::PF_*`).
    pub flag: u8,
    pub maxstacksize: u8,
    pub linedefined: i32,
    pub lastlinedefined: i32,
    pub(crate) k: RawBuf<Value>,
    pub(crate) code: RawBuf<u32>,
    pub(crate) p: RawBuf<*mut Proto>,
    pub(crate) upvalues: RawBuf<UpvalDesc>,
    pub(crate) lineinfo: RawBuf<i8>,
    pub(crate) abslineinfo: RawBuf<AbsLineInfo>,
    pub(crate) locvars: RawBuf<LocVar>,
    pub(crate) source: *mut LuaString,
    pub(crate) gclist: *mut GcHeader,
    /// JIT: compiled-code descriptor (null = not compiled). Only under the `jit` feature.
    #[cfg(feature = "jit")]
    pub jit: std::cell::Cell<*mut u8>,
    /// JIT: hotness counter (counts down from `jit_hooks::HOT_THRESHOLD`). Only under `jit`.
    #[cfg(feature = "jit")]
    pub hot: std::cell::Cell<i32>,
}

fn buf_slice<T: Copy>(b: &RawBuf<T>) -> &[T] {
    if b.cap() == 0 {
        &[]
    } else {
        // SAFETY: F4 — `cap` initialised elements at `as_ptr`.
        unsafe { std::slice::from_raw_parts(b.as_ptr(), b.cap()) }
    }
}

impl Proto {
    /// Constants.
    pub fn k(&self) -> &[Value] {
        buf_slice(&self.k)
    }
    /// Instructions.
    pub fn code(&self) -> &[u32] {
        buf_slice(&self.code)
    }
    /// Pointer to the first instruction (stable: protos never move).
    pub fn code_ptr(&self) -> *const u32 {
        self.code.as_ptr()
    }
    /// Pointer to the first constant.
    pub fn k_ptr(&self) -> *const Value {
        self.k.as_ptr()
    }
    pub fn upvalue_descs(&self) -> &[UpvalDesc] {
        buf_slice(&self.upvalues)
    }
    pub fn lineinfo(&self) -> &[i8] {
        buf_slice(&self.lineinfo)
    }
    pub fn abslineinfo(&self) -> &[AbsLineInfo] {
        buf_slice(&self.abslineinfo)
    }
    pub fn locvars(&self) -> &[LocVar] {
        buf_slice(&self.locvars)
    }
    /// Nested prototypes.
    pub fn protos(&self) -> &[Gc<Proto>] {
        let s = buf_slice(&self.p);
        // SAFETY: `Gc<Proto>` is a transparent wrapper of a non-null pointer; slots are non-null once
        // `from_data` finished (nested protos are NULL-filled only while building).
        unsafe { std::slice::from_raw_parts(s.as_ptr().cast::<Gc<Proto>>(), s.len()) }
    }
    /// Source name, if the chunk was not stripped.
    pub fn source(&self) -> Option<StrRef> {
        // a non-null source is a live string (F4)
        std::ptr::NonNull::new(self.source).map(StrRef)
    }
    /// `isvararg`.
    pub fn is_vararg(&self) -> bool {
        self.flag & (crate::protodata::PF_VAHID | crate::protodata::PF_VATAB) != 0
    }

    /// `luaF_protosize`.
    pub fn byte_size(&self) -> usize {
        std::mem::size_of::<Proto>()
            + self.p.cap() * std::mem::size_of::<*mut Proto>()
            + self.k.cap() * std::mem::size_of::<Value>()
            + self.locvars.cap() * std::mem::size_of::<LocVar>()
            + self.upvalues.cap() * std::mem::size_of::<UpvalDesc>()
            + self.code.cap() * 4
            + self.lineinfo.cap()
            + self.abslineinfo.cap() * std::mem::size_of::<AbsLineInfo>()
    }

    /// `luaF_getlocalname`: name of the `local_number`-th (1-based) active local at `pc`.
    pub fn local_name(&self, mut local_number: i32, pc: i32) -> Option<StrRef> {
        for lv in self.locvars() {
            if lv.startpc as i32 > pc {
                break;
            }
            if pc < lv.endpc as i32 {
                local_number -= 1;
                if local_number == 0 {
                    return Some(lv.name());
                }
            }
        }
        None
    }

    /// `luaG_getfuncline`: line of the instruction at `pc` (-1 without debug info).
    pub fn line_at(&self, pc: i32) -> i32 {
        const ABSLINEINFO: i8 = -0x80;
        const MAXIWTHABS: i32 = 128;
        let li = self.lineinfo();
        if li.is_empty() {
            return -1;
        }
        let abs = self.abslineinfo();
        let (mut basepc, mut baseline) = if abs.is_empty() || pc < abs[0].pc as i32 {
            (-1i32, self.linedefined)
        } else {
            let mut i = (pc / MAXIWTHABS - 1).max(0) as usize;
            while i + 1 < abs.len() && pc >= abs[i + 1].pc as i32 {
                i += 1;
            }
            (abs[i].pc as i32, abs[i].line)
        };
        while basepc < pc {
            basepc += 1;
            let d = li.get(basepc as usize).copied().unwrap_or(0);
            if d != ABSLINEINFO {
                baseline += i32::from(d);
            }
        }
        baseline
    }

    /// Build a prototype (and its nested ones) from compiler output.
    ///
    /// No garbage-collection safepoint occurs here, so the `StrRef`s inside `data` stay valid until
    /// the result is reachable. On `ErrMem` partially built protos are valid garbage for the next
    /// collection.
    pub fn from_data(heap: &mut Heap, data: &ProtoData<StrRef>) -> LuaResult<Gc<Proto>> {
        let p = heap.new_proto()?;
        // SAFETY: `p` is a fresh, fully initialised (empty) proto; arrays are sized then filled.
        unsafe {
            let pp = p.as_ptr();
            (*pp).numparams = data.num_params;
            (*pp).flag = data.flags;
            (*pp).maxstacksize = data.max_stack_size;
            (*pp).linedefined = data.line_defined;
            (*pp).lastlinedefined = data.last_line_defined;
            (*pp).source = data.source.map_or(std::ptr::null_mut(), StrRef::as_ptr);
            fill_buf(&mut (*pp).code, &mut heap.mem, &data.code)?;
            let ks: Vec<Value> = data
                .k
                .iter()
                .map(|c| match *c {
                    Const::Nil => Value::NIL,
                    Const::False => Value::FALSE,
                    Const::True => Value::TRUE,
                    Const::Int(i) => Value::int(i),
                    Const::Float(f) => Value::float(f),
                    Const::Str(s) => Value::string(s),
                })
                .collect();
            fill_buf(&mut (*pp).k, &mut heap.mem, &ks)?;
            let ups: Vec<UpvalDesc> = data
                .upvalues
                .iter()
                .map(|u| UpvalDesc {
                    name: u.name.map_or(std::ptr::null_mut(), StrRef::as_ptr),
                    instack: u.in_stack,
                    idx: u.idx,
                    kind: u.kind,
                })
                .collect();
            fill_buf(&mut (*pp).upvalues, &mut heap.mem, &ups)?;
            fill_buf(&mut (*pp).lineinfo, &mut heap.mem, &data.line_info)?;
            fill_buf(&mut (*pp).abslineinfo, &mut heap.mem, &data.abs_line_info)?;
            let lvs: Vec<LocVar> = data
                .loc_vars
                .iter()
                .map(|l| LocVar {
                    varname: l.name.as_ptr(),
                    startpc: l.start_pc,
                    endpc: l.end_pc,
                })
                .collect();
            fill_buf(&mut (*pp).locvars, &mut heap.mem, &lvs)?;
            // nested protos: NULL-fill first so a failure leaves a traversable proto
            (*pp).p.try_resize(&mut heap.mem, data.protos.len())?;
            for i in 0..data.protos.len() {
                (*pp).p.as_ptr().add(i).write(std::ptr::null_mut());
            }
            for (i, sub) in data.protos.iter().enumerate() {
                let c = Proto::from_data(heap, sub)?;
                (*pp).p.as_ptr().add(i).write(c.as_ptr());
            }
        }
        Ok(p)
    }
}

/// Resize `buf` to `src.len()` and copy.
fn fill_buf<T: Copy>(buf: &mut RawBuf<T>, mem: &mut Mem, src: &[T]) -> LuaResult<()> {
    buf.try_resize(mem, src.len())?;
    if !src.is_empty() {
        // SAFETY: the buffer has exactly `src.len()` slots; regions do not overlap.
        unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), buf.as_ptr(), src.len()) };
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// UpVal
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct OpenLinks {
    pub(crate) next: *mut UpVal,
    /// Address of the pointer that points to this upvalue (`&prev.next` or `&thread.openupval`).
    pub(crate) previous: *mut *mut UpVal,
}

#[repr(C)]
pub(crate) union UpValU {
    pub(crate) open: OpenLinks,
    pub(crate) value: Value,
}

/// `UpVal`.
#[repr(C)]
pub struct UpVal {
    pub(crate) hdr: GcHeader,
    /// Current value: a stack slot while open, `&u.value` when closed.
    pub(crate) v: *mut Value,
    pub(crate) u: UpValU,
}

impl Gc<UpVal> {
    /// `upisopen`.
    pub fn is_open(self) -> bool {
        // SAFETY: handle invariant; only addresses are compared.
        unsafe {
            let p = self.as_ptr();
            (*p).v != &raw mut (*p).u.value
        }
    }
    /// Current value of the upvalue.
    #[inline]
    pub fn get(self) -> Value {
        // SAFETY: F2/F3 — `v` points to a live slot.
        unsafe { Value::load((*self.as_ptr()).v) }
    }
    /// Store into the upvalue (the caller issues the barrier).
    #[inline]
    pub fn set(self, val: Value) {
        // SAFETY: F2/F3.
        unsafe { Value::store((*self.as_ptr()).v, val) }
    }
    /// Raw pointer to the current value slot (interpreter hot path; stale after a stack
    /// reallocation if the upvalue is open).
    #[inline]
    pub fn value_ptr(self) -> *mut Value {
        // SAFETY: handle invariant.
        unsafe { (*self.as_ptr()).v }
    }
}

// ---------------------------------------------------------------------------------------------
// closures
// ---------------------------------------------------------------------------------------------

/// Lua closure (`LClosure`); `nupvalues` upvalue pointers follow the struct.
#[repr(C)]
pub struct LClosure {
    // `GcHeader` layout (`next`, `tt`, `marked`) with the count in the spare bytes after it, as in the
    // reference (`ClosureHeader`): 8 bytes less per closure than embedding a full header.
    pub(crate) next: *mut GcHeader,
    pub(crate) tt: u8,
    pub(crate) marked: u8,
    pub(crate) nupvalues: u8,
    pub(crate) gclist: *mut GcHeader,
    pub(crate) p: *mut Proto,
    upvals: [*mut UpVal; 0],
}

/// Native closure (`CClosure`): a native function plus `nupvalues` values.
#[repr(C)]
pub struct CClosure {
    // `GcHeader` layout (`next`, `tt`, `marked`) with the count in the spare bytes after it, as in the
    // reference (`ClosureHeader`): 8 bytes less per closure than embedding a full header.
    pub(crate) next: *mut GcHeader,
    pub(crate) tt: u8,
    pub(crate) marked: u8,
    pub(crate) nupvalues: u8,
    pub(crate) gclist: *mut GcHeader,
    pub f: NativeFn,
    /// A C function pointer (`lua_CFunction`) when the closure was made by the C API (then `f` is
    /// the foreign-call trampoline), else null.
    pub(crate) cfn: *const (),
    upvalue: [Value; 0],
}

/// `sizeLclosure`.
pub(crate) fn size_lclosure(n: usize) -> usize {
    std::mem::size_of::<LClosure>() + n * std::mem::size_of::<*mut UpVal>()
}
/// `sizeCclosure`.
pub(crate) fn size_cclosure(n: usize) -> usize {
    std::mem::size_of::<CClosure>() + n * std::mem::size_of::<Value>()
}

impl LClosure {
    pub fn nupvalues(&self) -> usize {
        self.nupvalues as usize
    }
    /// The prototype.
    pub fn proto(&self) -> Gc<Proto> {
        // SAFETY: a closure being executed has its proto set (F1).
        unsafe { Gc::from_raw(self.p) }
    }
}

impl Gc<LClosure> {
    pub fn nupvalues(self) -> usize {
        self.nupvalues as usize
    }
    /// # Safety
    /// `i < nupvalues`.
    #[inline]
    unsafe fn slot(self, i: usize) -> *mut *mut UpVal {
        // SAFETY: caller contract; full-provenance pointer from the handle (F1).
        unsafe {
            (&raw mut (*self.as_ptr()).upvals)
                .cast::<*mut UpVal>()
                .add(i)
        }
    }
    /// The `i`-th upvalue (must be initialised).
    #[inline]
    pub fn upval(self, i: usize) -> Gc<UpVal> {
        assert!(i < self.nupvalues());
        // SAFETY: `i` in range; slots are non-null after construction (F1).
        unsafe { Gc::from_raw(*self.slot(i)) }
    }
    /// Raw upvalue pointers (interpreter hot path).
    #[inline]
    pub fn upvals_ptr(self) -> *mut *mut UpVal {
        // SAFETY: pointer to the trailing array.
        unsafe { (&raw mut (*self.as_ptr()).upvals).cast() }
    }
    /// Set the `i`-th upvalue (issues the barrier on `heap`).
    pub fn set_upval(self, heap: &mut Heap, i: usize, uv: Gc<UpVal>) {
        assert!(i < self.nupvalues());
        // SAFETY: in range (F1).
        unsafe {
            *self.slot(i) = uv.as_ptr();
            heap.barrier_obj(self.as_ptr().cast(), uv.as_ptr().cast());
        }
    }
    /// Install the prototype.
    ///
    /// # Safety
    /// `p.upvalue_descs().len()` must equal `nupvalues` (checked in debug builds).
    pub unsafe fn set_proto(self, p: Gc<Proto>) {
        debug_assert_eq!(p.upvalue_descs().len(), self.nupvalues());
        // SAFETY: handle invariant.
        unsafe { (*self.as_ptr()).p = p.as_ptr() }
    }
}

impl CClosure {
    pub fn nupvalues(&self) -> usize {
        self.nupvalues as usize
    }
}

impl Gc<CClosure> {
    pub fn nupvalues(self) -> usize {
        self.nupvalues as usize
    }
    /// The `i`-th upvalue value.
    pub fn upvalue(self, i: usize) -> Value {
        assert!(i < self.nupvalues());
        // SAFETY: in range; initialised at creation (F1).
        unsafe { *(&raw mut (*self.as_ptr()).upvalue).cast::<Value>().add(i) }
    }
    pub fn set_upvalue(self, heap: &mut Heap, i: usize, v: Value) {
        assert!(i < self.nupvalues());
        // SAFETY: in range (F1).
        unsafe {
            (&raw mut (*self.as_ptr()).upvalue)
                .cast::<Value>()
                .add(i)
                .write(v)
        };
        heap.barrier(self.as_ptr().cast(), &v);
    }
    /// `set_upvalue` of an integer: a plain store (no collectable value, so no write barrier is needed).
    #[inline(always)]
    pub fn set_upvalue_int(self, i: usize, n: i64) {
        assert!(i < self.nupvalues());
        // SAFETY: in range (F1); an integer is not a GC object.
        unsafe {
            (&raw mut (*self.as_ptr()).upvalue)
                .cast::<Value>()
                .add(i)
                .write(Value::int(n))
        };
    }
    pub fn native(self) -> NativeFn {
        self.f
    }
    /// The C function pointer of a closure made by the C API (null for Rust natives).
    pub fn cfn(self) -> *const () {
        self.cfn
    }
    /// Set the C function pointer (the closure's `f` must be the foreign-call trampoline).
    pub fn set_cfn(self, p: *const ()) {
        // SAFETY: handle invariant; plain data.
        unsafe { (*self.as_ptr()).cfn = p }
    }
}

impl Heap {
    /// `luaF_newproto`.
    pub fn new_proto(&mut self) -> LuaResult<Gc<Proto>> {
        let o = self.alloc_obj(std::mem::size_of::<Proto>(), LUA_VPROTO)?;
        let p = o.cast::<Proto>();
        // SAFETY: fresh block; header already initialised by `alloc_obj`; we write the rest.
        unsafe {
            (&raw mut (*p).numparams).write(0);
            (&raw mut (*p).flag).write(0);
            (&raw mut (*p).maxstacksize).write(0);
            (&raw mut (*p).linedefined).write(0);
            (&raw mut (*p).lastlinedefined).write(0);
            (&raw mut (*p).k).write(RawBuf::new());
            (&raw mut (*p).code).write(RawBuf::new());
            (&raw mut (*p).p).write(RawBuf::new());
            (&raw mut (*p).upvalues).write(RawBuf::new());
            (&raw mut (*p).lineinfo).write(RawBuf::new());
            (&raw mut (*p).abslineinfo).write(RawBuf::new());
            (&raw mut (*p).locvars).write(RawBuf::new());
            (&raw mut (*p).source).write(std::ptr::null_mut());
            (&raw mut (*p).gclist).write(std::ptr::null_mut());
            #[cfg(feature = "jit")]
            {
                (&raw mut (*p).jit).write(std::cell::Cell::new(std::ptr::null_mut()));
                (&raw mut (*p).hot).write(std::cell::Cell::new(crate::jit_hooks::initial_hot()));
            }
            Ok(Gc::from_raw(p))
        }
    }

    /// `luaF_newLclosure`: upvalue slots are NULL until filled.
    pub fn new_lclosure(&mut self, nup: usize) -> LuaResult<Gc<LClosure>> {
        if nup > MAXUPVAL {
            return Err(Status::ErrMem);
        }
        let o = self.alloc_obj(size_lclosure(nup), LUA_VLCL)?;
        let c = o.cast::<LClosure>();
        // SAFETY: fresh block of `size_lclosure(nup)` bytes.
        unsafe {
            (&raw mut (*c).nupvalues).write(nup as u8);
            (&raw mut (*c).gclist).write(std::ptr::null_mut());
            (&raw mut (*c).p).write(std::ptr::null_mut());
            let base = (&raw mut (*c).upvals).cast::<*mut UpVal>();
            for i in 0..nup {
                base.add(i).write(std::ptr::null_mut());
            }
            Ok(Gc::from_raw(c))
        }
    }

    /// `luaF_newCclosure`: upvalues start as nil.
    pub fn new_cclosure(&mut self, f: NativeFn, nup: usize) -> LuaResult<Gc<CClosure>> {
        if nup > MAXUPVAL {
            return Err(Status::ErrMem);
        }
        let o = self.alloc_obj(size_cclosure(nup), LUA_VCCL)?;
        let c = o.cast::<CClosure>();
        // SAFETY: fresh block of `size_cclosure(nup)` bytes.
        unsafe {
            (&raw mut (*c).nupvalues).write(nup as u8);
            (&raw mut (*c).gclist).write(std::ptr::null_mut());
            (&raw mut (*c).f).write(f);
            (&raw mut (*c).cfn).write(std::ptr::null());
            let base = (&raw mut (*c).upvalue).cast::<Value>();
            for i in 0..nup {
                base.add(i).write(Value::NIL);
            }
            Ok(Gc::from_raw(c))
        }
    }

    /// A closed upvalue holding `v`.
    pub fn new_upval_closed(&mut self, v: Value) -> LuaResult<Gc<UpVal>> {
        let o = self.alloc_obj(std::mem::size_of::<UpVal>(), LUA_VUPVAL)?;
        let uv = o.cast::<UpVal>();
        // SAFETY: fresh block.
        unsafe {
            (&raw mut (*uv).u).write(UpValU { value: v });
            (&raw mut (*uv).v).write(&raw mut (*uv).u.value);
            Ok(Gc::from_raw(uv))
        }
    }

    /// `luaF_initupvals`: fill a closure with fresh closed (nil) upvalues.
    pub fn init_upvals(&mut self, cl: Gc<LClosure>) -> LuaResult<()> {
        for i in 0..cl.nupvalues() {
            let uv = self.new_upval_closed(Value::NIL)?;
            // SAFETY: in range.
            unsafe {
                *cl.slot(i) = uv.as_ptr();
                self.barrier_obj(cl.as_ptr().cast(), uv.as_ptr().cast());
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// to-be-closed list
// ---------------------------------------------------------------------------------------------

/// Per-thread list of to-be-closed variables (stack indices in increasing order).
pub struct TbcList {
    buf: RawBuf<u32>,
    len: usize,
}

impl TbcList {
    /// Byte offset of the length word inside the list (tier 1 tests it: a `RETURN k` with no registered variable closes nothing).
    pub const LEN_OFFSET: usize = core::mem::offset_of!(TbcList, len);
    pub const fn new() -> Self {
        TbcList {
            buf: RawBuf::new(),
            len: 0,
        }
    }
    /// Highest registered stack index.
    pub fn last(&self) -> Option<usize> {
        if self.len == 0 {
            None
        } else {
            // SAFETY: `len <= cap`, slot initialised by `push`.
            Some(unsafe { *self.buf.as_ptr().add(self.len - 1) } as usize)
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn push(&mut self, mem: &mut Mem, idx: usize) -> LuaResult<()> {
        if self.len == self.buf.cap() {
            let ncap = (self.buf.cap() * 2).max(4);
            self.buf.try_resize(mem, ncap)?;
        }
        // SAFETY: `len < cap`.
        unsafe { self.buf.as_ptr().add(self.len).write(idx as u32) };
        self.len += 1;
        Ok(())
    }
    pub fn pop(&mut self) {
        self.len -= 1;
    }
    pub fn free(&mut self, mem: &mut Mem) {
        self.buf.free(mem);
        self.len = 0;
    }
}

impl Default for TbcList {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------------------------
// State: upvalues and closing
// ---------------------------------------------------------------------------------------------

impl State {
    /// Index of the stack slot an open upvalue points to (`uplevel`).
    fn uplevel(&self, uv: *mut UpVal) -> usize {
        // SAFETY: F2 — `v` points into this thread's stack buffer.
        unsafe { (*uv).v.offset_from(self.t().stack.as_ptr()) as usize }
    }

    /// `luaF_findupval`: find or create the open upvalue for the stack slot `level`.
    pub fn find_upval(&mut self, level: StkId) -> LuaResult<Gc<UpVal>> {
        let th = self.th_ptr();
        // SAFETY: F2/ST1 — walking and relinking the thread's open-upvalue list.
        unsafe {
            let mut pp: *mut *mut UpVal = &raw mut (*th).openupval;
            loop {
                let p = *pp;
                if p.is_null() || self.uplevel(p) < level.0 {
                    break;
                }
                if self.uplevel(p) == level.0 {
                    return Ok(Gc::from_raw(p));
                }
                pp = &raw mut (*p).u.open.next;
            }
            // not found: create a new upvalue after `pp`
            let o = self.try_alloc(|h| h.alloc_obj(std::mem::size_of::<UpVal>(), LUA_VUPVAL))?;
            let slot = self.slot_ptr(level); // after the allocation: it may have moved the stack
            let uv = o.cast::<UpVal>();
            let next = *pp;
            (&raw mut (*uv).v).write(slot);
            (&raw mut (*uv).u).write(UpValU {
                open: OpenLinks { next, previous: pp },
            });
            if !next.is_null() {
                (*next).u.open.previous = &raw mut (*uv).u.open.next;
            }
            *pp = uv;
            if (*th).twups == th {
                // thread not in the list of threads with upvalues: link it
                let g = &mut (*(*th).g).heap.gc;
                (*th).twups = g.twups;
                g.twups = th;
            }
            Ok(Gc::from_raw(uv))
        }
    }

    /// `luaF_closeupval`: close all open upvalues at or above `level`.
    pub fn close_upvals(&mut self, level: StkId) {
        let th = self.th_ptr();
        // SAFETY: ST1; the heap and the thread are disjoint objects.
        unsafe { close_upvals_raw(&mut (*(*th).g).heap, th, level.0) }
    }

    /// `luaF_newtbcupval`: mark the variable at `level` to be closed. `false`/`nil` need no closing.
    pub fn new_tbc_upval(&mut self, level: StkId) -> LuaResult<()> {
        debug_assert!(self.t().tbc.last().is_none_or(|l| l < level.0));
        if self.get(level).is_falsy() {
            return Ok(());
        }
        self.check_close_mth(level)?;
        let th = self.th_ptr();
        // SAFETY: ST1; `mem` and the thread's list are disjoint.
        unsafe { (*th).tbc.push(&mut (*(*th).g).heap.mem, level.0) }
    }

    /// `checkclosemth`.
    fn check_close_mth(&mut self, level: StkId) -> LuaResult<()> {
        let tm = self.get_tm_by_obj(&self.get(level), Tm::Close);
        if tm.is_nil() {
            let idx = level - self.ci().r().func;
            let vname = self
                .find_local_name(self.ci(), idx as i32)
                .unwrap_or_else(|| b"?".to_vec());
            let mut msg = b"variable '".to_vec();
            msg.extend_from_slice(&vname);
            msg.extend_from_slice(b"' got a non-closable value");
            return Err(self.run_error(&msg));
        }
        Ok(())
    }

    /// `callclosemethod`.
    fn call_close_method(&mut self, obj: Value, err: Option<Value>, yy: bool) -> LuaResult<()> {
        let top = self.top();
        let tm = self.get_tm_by_obj(&obj, Tm::Close);
        self.set(top, tm);
        self.set(top + 1, obj);
        let mut n = top + 2;
        if let Some(e) = err {
            self.set(n, e);
            n += 1;
        }
        self.set_top(n);
        if yy {
            self.call(top, 0)
        } else {
            self.call_no_yield(top, 0)
        }
    }

    /// `prepcallclosemth`.
    fn prep_call_close_mth(&mut self, level: StkId, status: u8, yy: bool) -> LuaResult<()> {
        let uv = self.get(level);
        // 5.4: the closing method always gets an error argument (nil when there is no error);
        // `luaD_seterrorobj(L, LUA_OK, level + 1)` puts that nil after the variable
        #[cfg(rustynail_lua54)]
        let errobj = match status {
            CLOSEKTOP => Some(Value::NIL),
            s => {
                if s == Status::Ok as u8 {
                    self.set(level + 1, Value::NIL);
                    self.set_top(level + 2);
                } else {
                    self.set_error_obj(status_from_u8(s), level + 1);
                }
                Some(self.get(level + 1))
            }
        };
        #[cfg(not(rustynail_lua54))]
        let errobj = match status {
            s if s == Status::Ok as u8 => {
                self.set_top(level + 1);
                None
            }
            CLOSEKTOP => None,
            s => {
                self.set_error_obj(status_from_u8(s), level + 1);
                Some(self.get(level + 1))
            }
        };
        self.call_close_method(uv, errobj, yy)
    }

    /// `luaF_close`: close upvalues and to-be-closed variables down to `level`. `status` is a
    /// `Status as u8`, or [`CLOSEKTOP`]. Returns the (unchanged, offsets are stable) `level`.
    pub fn close(&mut self, level: StkId, status: u8, yy: bool) -> LuaResult<StkId> {
        self.close_upvals(level);
        while let Some(tbc) = self.t().tbc.last() {
            if tbc < level.0 {
                break;
            }
            self.tm().tbc.pop();
            self.prep_call_close_mth(StkId(tbc), status, yy)?;
        }
        Ok(level)
    }

    /// `lua_closeslot` core: close the upvalues/tbc variables at `level` and above (keeping the
    /// top of the stack), then set the slot to nil.
    pub fn close_slot(&mut self, level: StkId) -> LuaResult<()> {
        self.close(level, CLOSEKTOP, false)?;
        self.set(level, Value::NIL);
        Ok(())
    }

    /// N7 (J5 recorder): whether `close(level)` would do nothing, i.e. no open upvalue points at or above stack index `level` and no
    /// to-be-closed variable lies there.
    pub fn jit_close_is_noop(&self, level: StkId) -> bool {
        let th = self.th_ptr();
        // SAFETY: ST3 — open upvalues point into the stack buffer; read only.
        let up_ok = unsafe {
            let uv = (*th).openupval;
            uv.is_null() || ((*uv).v.offset_from((*th).stack.as_ptr()) as usize) < level.0
        };
        up_ok && self.t().tbc.last().is_none_or(|t| t < level.0)
    }

    /// BUNDLE-S S1: is a to-be-closed variable registered at or above stack index `level`?
    pub fn jit_tbc_at_or_above(&self, level: StkId) -> bool {
        self.t().tbc.last().is_some_and(|t| t >= level.0)
    }

    /// Number of registered to-be-closed variables (tests/diagnostics).
    pub fn tbc_count(&self) -> usize {
        self.t().tbc.len()
    }
}

/// Map a numeric status code back to [`Status`].
pub(crate) fn status_from_u8(s: u8) -> Status {
    match s {
        0 => Status::Ok,
        1 => Status::Yield,
        2 => Status::ErrRun,
        3 => Status::ErrSyntax,
        4 => Status::ErrMem,
        _ => Status::ErrErr,
    }
}

/// The collector's `Proto` as seen by the (pure) debug-information code of [`crate::debuginfo`].
impl crate::debuginfo::ProtoView for Proto {
    fn code(&self) -> &[u32] {
        Proto::code(self)
    }
    fn k_str(&self, idx: usize) -> Option<&[u8]> {
        self.k()
            .get(idx)
            .and_then(|v| v.as_string())
            .map(|s| s.as_bytes())
    }
    fn upvalue_name(&self, idx: usize) -> Option<&[u8]> {
        self.upvalue_descs()
            .get(idx)
            .and_then(|u| u.name())
            .map(|s| s.get().as_bytes())
    }
    fn num_locvars(&self) -> usize {
        self.locvars().len()
    }
    fn locvar(&self, i: usize) -> (&[u8], i32, i32) {
        let v = &self.locvars()[i];
        (v.name().get().as_bytes(), v.startpc as i32, v.endpc as i32)
    }
    fn line_info(&self) -> &[i8] {
        self.lineinfo()
    }
    fn abs_line_info(&self) -> &[AbsLineInfo] {
        self.abslineinfo()
    }
    fn line_defined(&self) -> i32 {
        self.linedefined
    }
    fn last_line_defined(&self) -> i32 {
        self.lastlinedefined
    }
    fn source(&self) -> Option<&[u8]> {
        Proto::source(self).map(|s| s.get().as_bytes())
    }
    fn num_params(&self) -> u8 {
        self.numparams
    }
    fn flags(&self) -> u8 {
        self.flag
    }
}
