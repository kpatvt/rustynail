//! Tagged value representation (ADR-1; port of `TValue` in `lobject.h`).
//!
//! A [`Value`] is 16 bytes: a tag byte using the reference variant numbering (`LUA_V*`, with bit 6 set
//! for collectable payloads, i.e. `ctb(tt)`) and an 8-byte payload. `unsafe` is confined to this file's
//! payload-union reads, each justified by the tag check immediately before it.
//!
//! **Invariant (V1):** a `Value` whose tag says "collectable" holds a pointer to a live, correctly typed
//! GC object, as long as the owning `Heap` has not freed/collected it. Constructors taking raw
//! pointers are therefore `unsafe` unless they take a typed reference.
use crate::func::{CClosure, LClosure};
use crate::gc::GcHeader;
use crate::number::Number;
use crate::state::{NativeFn, Thread};
use crate::string::StrRef;
use crate::udata::Udata;
use std::ffi::c_void;

// Basic types (`lua.h`).
pub const LUA_TNONE: i8 = -1;
pub const LUA_TNIL: u8 = 0;
pub const LUA_TBOOLEAN: u8 = 1;
pub const LUA_TLIGHTUSERDATA: u8 = 2;
pub const LUA_TNUMBER: u8 = 3;
pub const LUA_TSTRING: u8 = 4;
pub const LUA_TTABLE: u8 = 5;
pub const LUA_TFUNCTION: u8 = 6;
pub const LUA_TUSERDATA: u8 = 7;
pub const LUA_TTHREAD: u8 = 8;
pub const LUA_NUMTYPES: u8 = 9;
/// Extra internal types (`lobject.h`).
pub const LUA_TUPVAL: u8 = LUA_NUMTYPES;
pub const LUA_TPROTO: u8 = LUA_NUMTYPES + 1;
pub const LUA_TDEADKEY: u8 = LUA_NUMTYPES + 2;

const fn makevariant(t: u8, v: u8) -> u8 {
    t | (v << 4)
}

// Variant tags (without the collectable bit), exactly as in `lobject.h`.
pub const LUA_VNIL: u8 = makevariant(LUA_TNIL, 0);
pub const LUA_VEMPTY: u8 = makevariant(LUA_TNIL, 1);
pub const LUA_VABSTKEY: u8 = makevariant(LUA_TNIL, 2);
/// 5.5 only: result of indexing a non-table (`LUA_VNOTABLE`).
pub const LUA_VNOTABLE: u8 = makevariant(LUA_TNIL, 3);
pub const LUA_VFALSE: u8 = makevariant(LUA_TBOOLEAN, 0);
pub const LUA_VTRUE: u8 = makevariant(LUA_TBOOLEAN, 1);
pub const LUA_VLIGHTUSERDATA: u8 = makevariant(LUA_TLIGHTUSERDATA, 0);
pub const LUA_VNUMINT: u8 = makevariant(LUA_TNUMBER, 0);
pub const LUA_VNUMFLT: u8 = makevariant(LUA_TNUMBER, 1);
pub const LUA_VSHRSTR: u8 = makevariant(LUA_TSTRING, 0);
pub const LUA_VLNGSTR: u8 = makevariant(LUA_TSTRING, 1);
pub const LUA_VTABLE: u8 = makevariant(LUA_TTABLE, 0);
pub const LUA_VLCL: u8 = makevariant(LUA_TFUNCTION, 0);
pub const LUA_VLCF: u8 = makevariant(LUA_TFUNCTION, 1);
/// Light *C* function (`lua_pushcfunction`): the payload is a raw `lua_CFunction` pointer, called
/// through the foreign-call trampoline installed by `rn-capi` ([`crate::call::set_c_trampoline`]).
pub const LUA_VLCFC: u8 = makevariant(LUA_TFUNCTION, 3);
pub const LUA_VCCL: u8 = makevariant(LUA_TFUNCTION, 2);
pub const LUA_VUSERDATA: u8 = makevariant(LUA_TUSERDATA, 0);
pub const LUA_VTHREAD: u8 = makevariant(LUA_TTHREAD, 0);
pub const LUA_VUPVAL: u8 = makevariant(LUA_TUPVAL, 0);
pub const LUA_VPROTO: u8 = makevariant(LUA_TPROTO, 0);

/// `BIT_ISCOLLECTABLE`.
pub const BIT_ISCOLLECTABLE: u8 = 1 << 6;

/// `ctb(t)`.
#[inline]
pub const fn ctb(t: u8) -> u8 {
    t | BIT_ISCOLLECTABLE
}

/// Typed handle to a GC object owned by a `Heap` (`Copy`; valid while the heap keeps the object alive).
/// `Deref` gives shared access to the fixed part of the object; trailing arrays (closure upvalues,
/// userdata payload) are reached only through the raw pointer from [`Gc::as_ptr`] (full provenance).
pub struct Gc<T>(std::ptr::NonNull<T>);

impl<T> Clone for Gc<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Gc<T> {}
impl<T> PartialEq for Gc<T> {
    fn eq(&self, o: &Self) -> bool {
        self.0 == o.0
    }
}
impl<T> Eq for Gc<T> {}
impl<T> std::fmt::Debug for Gc<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Gc({:p})", self.0)
    }
}
impl<T> Gc<T> {
    /// # Safety
    /// `p` must be a non-null pointer to a live object of type `T` owned by a heap.
    #[inline]
    pub unsafe fn from_raw(p: *mut T) -> Gc<T> {
        // SAFETY: caller guarantees non-null.
        Gc(unsafe { std::ptr::NonNull::new_unchecked(p) })
    }
    #[inline]
    pub fn as_ptr(self) -> *mut T {
        self.0.as_ptr()
    }
}
impl<T> std::ops::Deref for Gc<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: handle invariant (the object is alive while reachable/anchored).
        unsafe { self.0.as_ref() }
    }
}

/// User-visible Lua types (what `type()` reports).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LuaType {
    Nil = 0,
    Boolean,
    LightUserdata,
    Number,
    String,
    Table,
    Function,
    Userdata,
    Thread,
}

/// 8-byte payload shared by values and table nodes (a union, so pointer provenance survives copies).
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) union Payload {
    i: i64,
    n: f64,
    gc: *mut GcHeader,
    p: *mut c_void,
    f: *const (),
    bits: u64,
}

impl Payload {
    pub(crate) const ZERO: Payload = Payload { bits: 0 };
    /// # Safety
    /// The payload must have been written as an integer.
    #[inline]
    pub(crate) unsafe fn int(self) -> i64 {
        // SAFETY: caller contract.
        unsafe { self.i }
    }
    /// # Safety
    /// The payload must have been written as an object pointer.
    #[inline]
    pub(crate) unsafe fn gc(self) -> *mut GcHeader {
        // SAFETY: caller contract.
        unsafe { self.gc }
    }
    /// Address bits (for hashing pointers and diagnostics); always valid to read.
    #[inline]
    pub(crate) fn addr(self) -> usize {
        // SAFETY: every field is plain 8-byte data; reading the bits is always valid.
        unsafe { self.bits as usize }
    }
}

/// A Lua value: 16 bytes, `Copy`. See module docs.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Value {
    /// Tag, widened to 8 bytes so that whole-value copies and field-wise stores always overlap
    /// exactly (partial overlaps defeat store-to-load forwarding). Only the low byte is used.
    tagw: u64,
    payload: Payload,
}

impl Default for Value {
    fn default() -> Self {
        Value::NIL
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.tag() {
            LUA_VNIL => write!(f, "nil"),
            LUA_VEMPTY => write!(f, "<empty>"),
            LUA_VABSTKEY => write!(f, "<abstkey>"),
            LUA_VNOTABLE => write!(f, "<notable>"),
            LUA_VFALSE => write!(f, "false"),
            LUA_VTRUE => write!(f, "true"),
            LUA_VNUMINT => write!(f, "{}", self.as_int().unwrap_or(0)),
            LUA_VNUMFLT => write!(f, "{:?}", self.as_float().unwrap_or(0.0)),
            t => write!(f, "<tag {t:#04x} {:p}>", self.raw_ptr()),
        }
    }
}

impl Value {
    /// Load a value from memory as separate tag (1 byte) and payload (8 bytes) loads.
    ///
    /// Values are stored with a byte store for the tag and a qword store for the payload (as PUC
    /// does); loading the whole 16-byte aggregate right after such stores defeats store-to-load
    /// forwarding (a ~12 cycle stall, measured at 3x on table-heavy loops), so every hot-path load
    /// and store of a `Value` goes through [`Value::load`] / [`Value::store`].
    ///
    /// # Safety
    /// `p` must point to an initialised `Value`.
    #[inline(always)]
    pub unsafe fn load(p: *const Value) -> Value {
        // SAFETY: caller contract; `Value` is `repr(C)` with the payload at offset 8.
        unsafe {
            let tag = p.cast::<u8>().read();
            // read the payload as the union (not as an integer) so pointer provenance survives
            let payload = p.cast::<Payload>().byte_add(8).read();
            Value {
                tagw: tag as u64,
                payload,
            }
        }
    }

    /// Store a value as separate tag and payload stores (see [`Value::load`]).
    ///
    /// # Safety
    /// `p` must point to writable memory for a `Value`.
    #[inline(always)]
    pub unsafe fn store(p: *mut Value, v: Value) {
        // SAFETY: caller contract; the payload bits are plain data.
        unsafe {
            p.cast::<u8>().write(v.tag());
            p.cast::<Payload>().byte_add(8).write(v.payload);
        }
    }

    /// Read only the payload bits as an integer.
    ///
    /// # Safety
    /// `p` must point to an initialised `Value` (the caller knows it holds an integer).
    #[inline(always)]
    pub unsafe fn load_int_payload(p: *const Value) -> i64 {
        // SAFETY: caller contract; payload at offset 8.
        unsafe { p.cast::<i64>().add(1).read() }
    }

    /// Read only the payload bits as a float (straight into a float register: no integer-to-float
    /// register move on the critical path of float arithmetic).
    ///
    /// # Safety
    /// `p` must point to an initialised `Value` (the caller knows it holds a float).
    #[inline(always)]
    pub unsafe fn load_float_payload(p: *const Value) -> f64 {
        // SAFETY: caller contract; payload at offset 8.
        unsafe { p.cast::<f64>().add(1).read() }
    }

    /// Read only the tag byte.
    ///
    /// # Safety
    /// `p` must point to an initialised `Value`.
    #[inline(always)]
    pub unsafe fn load_tag(p: *const Value) -> u8 {
        // SAFETY: caller contract; the tag is the first byte.
        unsafe { p.cast::<u8>().read() }
    }

    /// Overwrite only the payload bits (the tag must already be the right one).
    ///
    /// # Safety
    /// `p` must point to a `Value` whose tag is already integer.
    #[inline(always)]
    pub unsafe fn store_int_payload(p: *mut Value, i: i64) {
        // SAFETY: caller contract.
        unsafe { p.cast::<i64>().add(1).write(i) }
    }

    pub const NIL: Value = Value {
        tagw: (LUA_VNIL) as u64,
        payload: Payload { bits: 0 },
    };
    /// Empty table slot marker (`LUA_VEMPTY`).
    pub const EMPTY: Value = Value {
        tagw: (LUA_VEMPTY) as u64,
        payload: Payload { bits: 0 },
    };
    /// Absent-key marker (`LUA_VABSTKEY`).
    pub const ABSTKEY: Value = Value {
        tagw: (LUA_VABSTKEY) as u64,
        payload: Payload { bits: 0 },
    };
    pub const FALSE: Value = Value {
        tagw: (LUA_VFALSE) as u64,
        payload: Payload { bits: 0 },
    };
    pub const TRUE: Value = Value {
        tagw: (LUA_VTRUE) as u64,
        payload: Payload { bits: 0 },
    };

    // ---- constructors -------------------------------------------------------------------------

    #[inline]
    pub const fn int(i: i64) -> Value {
        Value {
            tagw: (LUA_VNUMINT) as u64,
            payload: Payload { i },
        }
    }

    #[inline]
    pub const fn float(n: f64) -> Value {
        Value {
            tagw: (LUA_VNUMFLT) as u64,
            payload: Payload { n },
        }
    }

    #[inline]
    pub const fn boolean(b: bool) -> Value {
        if b {
            Value::TRUE
        } else {
            Value::FALSE
        }
    }

    #[inline]
    pub fn number(n: Number) -> Value {
        match n {
            Number::Int(i) => Value::int(i),
            Number::Float(f) => Value::float(f),
        }
    }

    #[inline]
    pub const fn light_userdata(p: *mut c_void) -> Value {
        Value {
            tagw: (LUA_VLIGHTUSERDATA) as u64,
            payload: Payload { p },
        }
    }

    /// Light native function value. The payload of every `LUA_VLCF` value is a [`NativeFn`] (this is
    /// the only constructor), which is what makes [`Value::as_native`] sound.
    #[inline]
    pub fn native(f: NativeFn) -> Value {
        Value {
            tagw: (LUA_VLCF) as u64,
            payload: Payload { f: f as *const () },
        }
    }

    /// Light C function value (`lua_pushcfunction`); `f` is a `lua_CFunction` pointer.
    #[inline]
    pub fn light_cfunction(f: *const ()) -> Value {
        Value {
            tagw: (LUA_VLCFC) as u64,
            payload: Payload { f },
        }
    }
    /// The C function pointer of a light C function value.
    #[inline]
    pub fn as_light_cfn(&self) -> Option<*const ()> {
        if self.tag() == LUA_VLCFC {
            // SAFETY: the tag says the payload is the raw function pointer.
            Some(unsafe { self.payload.f })
        } else {
            None
        }
    }

    /// Closure/userdata/thread constructors (typed, hence safe: the handle proves liveness, V1).
    #[inline]
    pub fn lclosure(c: Gc<LClosure>) -> Value {
        Value {
            tagw: (ctb(LUA_VLCL)) as u64,
            payload: Payload {
                gc: c.as_ptr().cast(),
            },
        }
    }
    #[inline]
    pub fn cclosure(c: Gc<CClosure>) -> Value {
        Value {
            tagw: (ctb(LUA_VCCL)) as u64,
            payload: Payload {
                gc: c.as_ptr().cast(),
            },
        }
    }
    #[inline]
    pub fn userdata(u: Gc<Udata>) -> Value {
        Value {
            tagw: (ctb(LUA_VUSERDATA)) as u64,
            payload: Payload {
                gc: u.as_ptr().cast(),
            },
        }
    }
    #[inline]
    pub fn thread(t: Gc<Thread>) -> Value {
        Value {
            tagw: (ctb(LUA_VTHREAD)) as u64,
            payload: Payload {
                gc: t.as_ptr().cast(),
            },
        }
    }

    /// Typed accessors (None when the tag does not match).
    #[inline]
    pub fn as_lclosure(&self) -> Option<Gc<LClosure>> {
        if self.tag() == ctb(LUA_VLCL) {
            // SAFETY: tag says the payload is a live `LClosure` (V1).
            Some(unsafe { Gc::from_raw(self.payload.gc.cast()) })
        } else {
            None
        }
    }
    #[inline]
    pub fn as_cclosure(&self) -> Option<Gc<CClosure>> {
        if self.tag() == ctb(LUA_VCCL) {
            // SAFETY: tag says the payload is a live `CClosure` (V1).
            Some(unsafe { Gc::from_raw(self.payload.gc.cast()) })
        } else {
            None
        }
    }
    #[inline]
    pub fn as_userdata(&self) -> Option<Gc<Udata>> {
        if self.tag() == ctb(LUA_VUSERDATA) {
            // SAFETY: tag says the payload is a live `Udata` (V1).
            Some(unsafe { Gc::from_raw(self.payload.gc.cast()) })
        } else {
            None
        }
    }
    #[inline]
    pub fn as_thread(&self) -> Option<Gc<Thread>> {
        if self.tag() == ctb(LUA_VTHREAD) {
            // SAFETY: tag says the payload is a live `Thread` (V1).
            Some(unsafe { Gc::from_raw(self.payload.gc.cast()) })
        } else {
            None
        }
    }
    /// The native function of a light native function value.
    #[inline]
    pub fn as_native(&self) -> Option<NativeFn> {
        if self.tag() == LUA_VLCF {
            // SAFETY: LUA_VLCF payloads are only created by `Value::native` from a `NativeFn`.
            Some(unsafe { std::mem::transmute::<*const (), NativeFn>(self.payload.f) })
        } else {
            None
        }
    }
    #[inline]
    pub const fn is_lclosure(&self) -> bool {
        self.tag() == ctb(LUA_VLCL)
    }
    #[inline]
    pub const fn is_cclosure(&self) -> bool {
        self.tag() == ctb(LUA_VCCL)
    }
    #[inline]
    pub const fn is_userdata(&self) -> bool {
        self.tag() == ctb(LUA_VUSERDATA)
    }
    #[inline]
    pub const fn is_thread(&self) -> bool {
        self.tag() == ctb(LUA_VTHREAD)
    }

    /// Value for a string object created by a `Heap` (tag chosen from the object's kind).
    #[inline]
    pub fn string(s: StrRef) -> Value {
        let tag = ctb(if s.get().is_short() {
            LUA_VSHRSTR
        } else {
            LUA_VLNGSTR
        });
        Value {
            tagw: tag as u64,
            payload: Payload {
                gc: s.as_ptr().cast(),
            },
        }
    }

    /// Collectable value from a raw object pointer.
    ///
    /// # Safety
    /// `gc` must point to a live GC object whose header `tt` equals `variant` (invariant V1).
    #[inline]
    pub unsafe fn from_gc(variant: u8, gc: *mut GcHeader) -> Value {
        Value {
            tagw: (ctb(variant)) as u64,
            payload: Payload { gc },
        }
    }

    /// Split into (tag, payload) for compact storage (table nodes).
    #[inline]
    pub(crate) fn to_parts(self) -> (u8, Payload) {
        (self.tag(), self.payload)
    }

    /// Rebuild a value from stored parts.
    ///
    /// # Safety
    /// `(tag, payload)` must come from `to_parts` of a value that is still valid (invariant V1).
    #[inline]
    pub(crate) unsafe fn from_parts(tag: u8, payload: Payload) -> Value {
        Value {
            tagw: tag as u64,
            payload,
        }
    }

    /// Value for a table created by a `Heap`.
    #[inline]
    pub fn table(t: crate::table::TableRef) -> Value {
        Value {
            tagw: (ctb(LUA_VTABLE)) as u64,
            payload: Payload {
                gc: t.as_ptr().cast(),
            },
        }
    }

    /// Table handle of a value whose tag the caller checked (no `Option`: LLVM keeps a null test
    /// of the `Option<TableRef>` niche after the tag test of [`Value::as_table`]).
    ///
    /// # Safety
    /// The tag must be `ctb(LUA_VTABLE)`.
    #[inline(always)]
    pub unsafe fn table_unchecked(&self) -> crate::table::TableRef {
        debug_assert!(self.is_table());
        // SAFETY: caller contract; table tags carry a non-null pointer to a live `Table` (V1).
        unsafe { crate::table::TableRef::from_raw(self.payload.gc.cast()) }
    }

    /// String handle of a value whose tag the caller checked (see [`Value::table_unchecked`]).
    ///
    /// # Safety
    /// The value must be a string.
    #[inline(always)]
    pub unsafe fn strref_unchecked(&self) -> StrRef {
        debug_assert!(self.is_string());
        // SAFETY: caller contract; string tags carry a non-null pointer to a live `LuaString`.
        StrRef(unsafe { std::ptr::NonNull::new_unchecked(self.payload.gc.cast()) })
    }

    /// Table, if the value is one (valid while the heap keeps it alive).
    #[inline]
    pub fn as_table(&self) -> Option<crate::table::TableRef> {
        if self.tag() == ctb(LUA_VTABLE) {
            // SAFETY: table tags carry a non-null pointer to a live `Table` (V1).
            Some(unsafe { crate::table::TableRef::from_raw(self.payload.gc.cast()) })
        } else {
            None
        }
    }

    // ---- tag queries --------------------------------------------------------------------------

    /// The raw tag byte (variant | collectable bit).
    #[inline]
    pub const fn tag(&self) -> u8 {
        self.tagw as u8
    }

    /// Variant without the collectable bit (`withvariant`).
    #[inline]
    pub const fn variant(&self) -> u8 {
        self.tag() & 0x3F
    }

    /// Basic type number (`ttype`), 0..=8 for valid user values.
    #[inline]
    pub const fn type_tag(&self) -> u8 {
        self.tag() & 0x0F
    }

    /// `lua_type`-style user type.
    #[inline]
    pub fn type_of(&self) -> LuaType {
        match self.type_tag() {
            LUA_TNIL => LuaType::Nil,
            LUA_TBOOLEAN => LuaType::Boolean,
            LUA_TLIGHTUSERDATA => LuaType::LightUserdata,
            LUA_TNUMBER => LuaType::Number,
            LUA_TSTRING => LuaType::String,
            LUA_TTABLE => LuaType::Table,
            LUA_TFUNCTION => LuaType::Function,
            LUA_TUSERDATA => LuaType::Userdata,
            LUA_TTHREAD => LuaType::Thread,
            // internal types (upvalue/proto/dead key) never appear as user values
            _ => LuaType::Nil,
        }
    }

    /// `ttisnil` (any nil variant, including `EMPTY`/`ABSTKEY`).
    #[inline]
    pub const fn is_nil(&self) -> bool {
        self.type_tag() == LUA_TNIL
    }
    /// `isempty`.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.type_tag() == LUA_TNIL
    }
    /// `ttisstrictnil`.
    #[inline]
    pub const fn is_strict_nil(&self) -> bool {
        self.tag() == LUA_VNIL
    }
    #[inline]
    pub const fn is_abstkey(&self) -> bool {
        self.tag() == LUA_VABSTKEY
    }
    /// `l_isfalse`: nil or false.
    #[inline]
    pub const fn is_falsy(&self) -> bool {
        self.tag() == LUA_VFALSE || self.type_tag() == LUA_TNIL
    }
    #[inline]
    pub const fn is_truthy(&self) -> bool {
        !self.is_falsy()
    }
    #[inline]
    pub const fn is_boolean(&self) -> bool {
        self.type_tag() == LUA_TBOOLEAN
    }
    #[inline]
    pub const fn is_number(&self) -> bool {
        self.type_tag() == LUA_TNUMBER
    }
    #[inline]
    pub const fn is_int(&self) -> bool {
        self.tag() == LUA_VNUMINT
    }
    #[inline]
    pub const fn is_float(&self) -> bool {
        self.tag() == LUA_VNUMFLT
    }
    #[inline]
    pub const fn is_string(&self) -> bool {
        self.type_tag() == LUA_TSTRING
    }
    #[inline]
    pub const fn is_short_string(&self) -> bool {
        self.variant() == LUA_VSHRSTR
    }
    #[inline]
    pub const fn is_long_string(&self) -> bool {
        self.variant() == LUA_VLNGSTR
    }
    #[inline]
    pub const fn is_table(&self) -> bool {
        self.tag() == ctb(LUA_VTABLE)
    }
    #[inline]
    pub const fn is_function(&self) -> bool {
        self.type_tag() == LUA_TFUNCTION
    }
    #[inline]
    pub const fn is_light_userdata(&self) -> bool {
        self.tag() == LUA_VLIGHTUSERDATA
    }
    #[inline]
    pub const fn is_light_cfunction(&self) -> bool {
        self.tag() == LUA_VLCF || self.tag() == LUA_VLCFC
    }
    /// `iscollectable`.
    #[inline]
    pub const fn is_collectable(&self) -> bool {
        self.tag() & BIT_ISCOLLECTABLE != 0
    }

    // ---- accessors (None when the tag does not match) -----------------------------------------

    #[inline]
    pub fn as_int(&self) -> Option<i64> {
        if self.tag() == LUA_VNUMINT {
            // SAFETY: tag says the payload was written as `i`.
            Some(unsafe { self.payload.i })
        } else {
            None
        }
    }

    #[inline]
    pub fn as_float(&self) -> Option<f64> {
        if self.tag() == LUA_VNUMFLT {
            // SAFETY: tag says the payload was written as `n`.
            Some(unsafe { self.payload.n })
        } else {
            None
        }
    }

    #[inline]
    pub fn as_number(&self) -> Option<Number> {
        match self.tag() {
            LUA_VNUMINT => {
                // SAFETY: the tag identifies the active union field.
                Some(Number::Int(unsafe { self.payload.i }))
            }
            LUA_VNUMFLT => {
                // SAFETY: the tag identifies the active union field.
                Some(Number::Float(unsafe { self.payload.n }))
            }
            _ => None,
        }
    }

    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        match self.tag() {
            LUA_VFALSE => Some(false),
            LUA_VTRUE => Some(true),
            _ => None,
        }
    }

    #[inline]
    pub fn as_light_userdata(&self) -> Option<*mut c_void> {
        if self.tag() == LUA_VLIGHTUSERDATA {
            // SAFETY: tag identifies field `p`.
            Some(unsafe { self.payload.p })
        } else {
            None
        }
    }

    /// The GC object header pointer if the value is collectable.
    #[inline]
    pub fn as_gc(&self) -> Option<*mut GcHeader> {
        if self.is_collectable() {
            // SAFETY: collectable tags always carry a `gc` payload.
            Some(unsafe { self.payload.gc })
        } else {
            None
        }
    }

    /// String object, if the value is a string.
    ///
    /// The returned reference is valid only while the owning heap keeps the object alive (invariant V1);
    /// callers must not hold it across a point where the string may be collected.
    #[inline]
    pub fn as_string(&self) -> Option<crate::string::StrView<'_>> {
        self.as_strref().map(StrRef::get)
    }

    /// String handle, if the value is a string (`StrRef` is a copyable handle; see invariant S3).
    #[inline]
    pub fn as_strref(&self) -> Option<StrRef> {
        if self.is_string() {
            // SAFETY: string tags carry a non-null pointer to a live `LuaString` (V1).
            Some(StrRef(unsafe {
                std::ptr::NonNull::new_unchecked(self.payload.gc.cast())
            }))
        } else {
            None
        }
    }

    /// Payload bits for diagnostics only.
    #[inline]
    fn raw_ptr(&self) -> *const () {
        // SAFETY: every payload field is plain 8-byte data; reading it as bits is always valid.
        unsafe { self.payload.bits as *const () }
    }

    /// BUNDLE-C: the two raw words of the value (tag word, payload bits), for memoising a comparison against a model: equal
    /// words mean the same value for every purpose of the recorder's lockstep (unused payload bytes may differ for equal values,
    /// which only makes a memo miss).
    #[inline]
    pub fn raw_words(&self) -> (u64, u64) {
        // SAFETY: every payload field is plain 8-byte data (see `Payload::addr`).
        (self.tagw, unsafe { self.payload.bits })
    }

    // ---- equality -----------------------------------------------------------------------------

    /// `luaV_equalobj` without metamethods (`lua_rawequal`): numbers compare by value across int/float,
    /// strings by content, other objects by identity.
    pub fn raw_equal(&self, other: &Value) -> bool {
        if self.tag() != other.tag() {
            // Only int/float may compare equal across variants (nil variants do not).
            return match (self.as_number(), other.as_number()) {
                (Some(a), Some(b)) => crate::number::eq_num(a, b),
                _ => false,
            };
        }
        match self.tag() {
            LUA_VNIL | LUA_VEMPTY | LUA_VABSTKEY | LUA_VNOTABLE | LUA_VFALSE | LUA_VTRUE => true,
            // SAFETY (each arm): both tags are equal and identify the union field read.
            LUA_VNUMINT => {
                // SAFETY: see above.
                unsafe { self.payload.i == other.payload.i }
            }
            LUA_VNUMFLT => {
                // SAFETY: see above.
                unsafe { self.payload.n == other.payload.n }
            }
            LUA_VLIGHTUSERDATA => {
                // SAFETY: see above.
                unsafe { self.payload.p == other.payload.p }
            }
            LUA_VLCF | LUA_VLCFC => {
                // SAFETY: see above.
                unsafe { self.payload.f == other.payload.f }
            }
            t if t == ctb(LUA_VLNGSTR) => match (self.as_string(), other.as_string()) {
                (Some(a), Some(b)) => a.as_bytes() == b.as_bytes(),
                _ => false,
            },
            // short strings are interned; tables, userdata, closures, threads: identity
            // SAFETY: remaining tags are all collectable (or light C closures handled above).
            _ => unsafe { self.payload.gc == other.payload.gc },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_16() {
        assert_eq!(std::mem::size_of::<Value>(), 16);
        assert_eq!(std::mem::align_of::<Value>(), 8);
    }

    #[test]
    fn tags_match_reference_numbering() {
        assert_eq!(LUA_VNIL, 0);
        assert_eq!(LUA_VEMPTY, 0x10);
        assert_eq!(LUA_VABSTKEY, 0x20);
        assert_eq!(LUA_VFALSE, 1);
        assert_eq!(LUA_VTRUE, 0x11);
        assert_eq!(LUA_VNUMINT, 3);
        assert_eq!(LUA_VNUMFLT, 0x13);
        assert_eq!(LUA_VSHRSTR, 4);
        assert_eq!(LUA_VLNGSTR, 0x14);
        assert_eq!(LUA_VLCL, 6);
        assert_eq!(LUA_VLCF, 0x16);
        assert_eq!(LUA_VLCFC, 0x36);
        assert_eq!(LUA_VCCL, 0x26);
        assert_eq!(ctb(LUA_VTABLE), 0x45);
    }

    #[test]
    fn basics() {
        assert!(Value::NIL.is_falsy() && Value::FALSE.is_falsy() && Value::EMPTY.is_falsy());
        assert!(
            Value::TRUE.is_truthy() && Value::int(0).is_truthy() && Value::float(0.0).is_truthy()
        );
        assert_eq!(Value::int(5).as_int(), Some(5));
        assert_eq!(Value::int(5).as_float(), None);
        assert_eq!(Value::float(2.5).as_number(), Some(Number::Float(2.5)));
        assert_eq!(Value::int(1).type_of(), LuaType::Number);
        assert_eq!(Value::NIL.type_of(), LuaType::Nil);
        assert_eq!(Value::boolean(true).type_of(), LuaType::Boolean);
        assert!(Value::EMPTY.is_nil() && !Value::EMPTY.is_strict_nil());
        assert!(!Value::int(1).is_collectable());
    }

    #[test]
    fn raw_equal_numbers() {
        assert!(Value::int(1).raw_equal(&Value::float(1.0)));
        assert!(!Value::int(1).raw_equal(&Value::float(1.5)));
        assert!(!Value::float(f64::NAN).raw_equal(&Value::float(f64::NAN)));
        assert!(Value::float(0.0).raw_equal(&Value::float(-0.0)));
        assert!(!Value::NIL.raw_equal(&Value::EMPTY));
        assert!(Value::TRUE.raw_equal(&Value::TRUE));
        assert!(!Value::TRUE.raw_equal(&Value::FALSE));
        let mut x = 0u8;
        let p = (&mut x as *mut u8).cast();
        assert!(Value::light_userdata(p).raw_equal(&Value::light_userdata(p)));
        assert!(!Value::light_userdata(p).raw_equal(&Value::int(1)));
    }
}
