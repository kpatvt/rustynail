//! Layout descriptors for the JIT (docs/JIT_PLAN.md, decision D2): **the single source of layout truth**
//! for machine code that reads or writes interpreter objects.
//!
//! Everything here is a `pub const` (or `pub fn` where the layout is not a compile-time fact) computed with
//! `core::mem::offset_of!` / `size_of`, so no field has to become public and no offset is ever written by
//! hand in `rn-jit`. Offsets are in bytes from the start of the object. The unit tests at the bottom pin
//! the `repr(C)` expectations (48-byte `Table`, 16-byte `Value`, ...) and check the derived offsets against
//! real objects, so a layout change fails here instead of in generated code.
//!
//! Version differences (5.4 vs 5.5): the object layouts below are identical in both builds. What differs is
//! behaviour that machine code must honour, exposed as constants: [`LUA54`], [`TABLE_ALL_HASH_HAVE_LASTFREE`]
//! and [`ARRAY_ALIMIT_TOUCH`] (5.4 keeps an array limit hint in the length-hint slot, so an inline array
//! access must go through the helper there).
use crate::func::{LClosure, Proto, UpVal};
use crate::gc::GcHeader;
use crate::state::CallInfo;
use crate::string::LuaString;
use crate::table::{Node, Table};
use crate::value::{Payload, Value};
use core::mem::{align_of, offset_of, size_of};

/// True in the Lua 5.4 build (`--cfg rustynail_lua54`).
pub const LUA54: bool = cfg!(rustynail_lua54);
/// 5.4: every non-dummy hash part carries a `lastfree` pointer; 5.5: only parts with at least 8 nodes.
pub const TABLE_ALL_HASH_HAVE_LASTFREE: bool = LUA54;
/// 5.4: array accesses update the `alimit` hint (`alimit_touch`), so templates must not inline them blindly.
pub const ARRAY_ALIMIT_TOUCH: bool = LUA54;

const fn align_up(x: usize, a: usize) -> usize {
    x.div_ceil(a) * a
}

/// `Value`: 16 bytes, tag word (only the low byte is used) then payload.
pub mod value {
    use super::*;
    /// `size_of::<Value>()` = 16.
    pub const SIZE: usize = size_of::<Value>();
    /// Offset of the 8-byte tag word (the tag is its low byte; the upper bytes are always zero). The fields
    /// are private to `value.rs`; the offsets follow from `repr(C)` (`tagw: u64`, then `payload`) and are
    /// checked against a real `Value` in the tests.
    pub const TAG: usize = 0;
    /// Offset of the 8-byte payload (integer, float bits, or object pointer).
    pub const PAYLOAD: usize = align_up(size_of::<u64>(), align_of::<Payload>());
    const _: () = assert!(PAYLOAD + size_of::<Payload>() == SIZE);
    /// Size of the payload union.
    pub const PAYLOAD_SIZE: usize = size_of::<Payload>();
}

/// `Table` (48 bytes; `GcHeader`-compatible head).
pub mod table {
    use super::*;
    /// `size_of::<Table>()` = 48.
    pub const SIZE: usize = size_of::<Table>();
    /// GC `marked` byte.
    pub const MARKED: usize = offset_of!(Table, marked);
    /// `flags`: metamethod-absence cache bits plus `BITDUMMY` (`1 << 6`) for the shared dummy node.
    pub const FLAGS: usize = offset_of!(Table, flags);
    /// `lsizenode`: log2 of the node count (byte).
    pub const LSIZENODE: usize = offset_of!(Table, lsizenode);
    /// `asize`: array-part size (`u32`).
    pub const ASIZE: usize = offset_of!(Table, asize);
    /// Array pointer (null iff `asize == 0`). The field is private to `table.rs`; its offset follows from the
    /// `repr(C)` order (`asize` is followed by `array`, `node`, `metatable`) and is asserted below.
    pub const ARRAY: usize = align_up(ASIZE + size_of::<u32>(), align_of::<*mut u8>());
    /// Node-array pointer (the shared dummy node when `flags & BITDUMMY`).
    pub const NODE: usize = ARRAY + size_of::<*mut u8>();
    /// Metatable pointer (null = none).
    pub const METATABLE: usize = offset_of!(Table, metatable);
    /// `flags` bit: the hash part is the dummy node.
    pub const BITDUMMY: u8 = crate::table::BITDUMMY;
    /// 5.4 only: `flags` bit meaning the array limit (`alimit`, kept in the `u32` length-hint slot at
    /// `array + 0`) is not the real array size. An integer access to array index `u < asize` with
    /// `BITRAS` set and `u >= hint` must perform `alimit_touch` (see `table.rs`).
    #[cfg(rustynail_lua54)]
    pub const BITRAS: u8 = crate::table::BITRAS;
    /// Offset of the `u32` length hint / 5.4 array limit from the `array` pointer.
    pub const LENHINT: usize = 0;
    /// Array part (invariant T1): the pointer `array` addresses the `u32` length hint; value payloads are 8
    /// bytes each stored *below* it (slot `k`, 0-based, at `array - 8 * (k + 1)`), tag bytes above it (slot
    /// `k` at `array + 4 + k`). Bytes between consecutive tags: 1; between payloads: 8.
    pub const ARRAY_TAG_BASE: usize = 4;
    /// Stride of one array payload (stored at negative offsets).
    pub const ARRAY_PAYLOAD_STRIDE: usize = 8;
    const _: () = assert!(NODE + size_of::<*mut u8>() == METATABLE);
    const _: () = assert!(ASIZE + size_of::<u32>() <= ARRAY);
}

/// Hash `Node` (24 bytes): value payload, value tag, key tag, chain offset (in nodes), key payload.
pub mod node {
    use super::*;
    /// `size_of::<Node>()` = 24.
    pub const SIZE: usize = size_of::<Node>();
    /// Value payload (8 bytes) at offset 0.
    pub const VAL: usize = 0;
    /// Value tag byte.
    pub const VAL_TT: usize = size_of::<Payload>();
    /// Key tag byte.
    pub const KEY_TT: usize = VAL_TT + 1;
    /// `next`: signed `i32` chain offset in *nodes* (0 = end of chain).
    pub const NEXT: usize = align_up(KEY_TT + 1, align_of::<i32>());
    /// Key payload.
    pub const KEY: usize = align_up(NEXT + size_of::<i32>(), align_of::<Payload>());
    const _: () = assert!(KEY + size_of::<Payload>() == SIZE);
    /// A1: the `lastfree` pointer of a hash part that has one lives this many bytes before the first node.
    pub const LASTFREE_BYTES: usize = crate::table::LASTFREE_BYTES;
    const _: () = assert!(align_of::<Node>() == align_of::<Payload>());
}

/// `LuaString` fields the templates need.
pub mod string {
    use super::*;
    /// Offset of the 32-bit hash of a *short* string (a spare field of the `GcHeader`).
    pub const SHORT_HASH: usize = offset_of!(LuaString, hdr) + offset_of!(GcHeader, hash);
    /// BUNDLE-J (hash-D): offset of `extra` of the header: 0 = the PUC hash of the short string was never computed, so it is not a key of any table
    /// (a lookup of it is a miss without reading the node vector).
    pub const EXTRA: usize = offset_of!(LuaString, hdr) + offset_of!(GcHeader, extra);
    /// N5: offset of the variant tag byte `tt` of the object header (`LUA_VSHRSTR` / `LUA_VLNGSTR`, no collectable bit).
    pub const TT: usize = offset_of!(LuaString, hdr) + offset_of!(GcHeader, tt);
    /// P-B: offset of `shrlen` (the length byte of a *short* string).
    pub const SHRLEN: usize = offset_of!(LuaString, hdr) + offset_of!(GcHeader, shrlen);
    /// F2: offset of the bytes of a string whose contents are inline (every short string, a long one with `shrlen == -1`).
    pub const CONTENTS: usize = crate::string::CONTENTS_OFFSET;
    /// P-B: offset of the 64-bit length of a *long* string (`u.len`; the first word of the union).
    pub const LNGLEN: usize = offset_of!(LuaString, u);
}

/// `UpVal` fields.
pub mod upval {
    use super::*;
    /// Offset of `v`: pointer to the current value (a stack slot while open, the upvalue's own storage when
    /// closed).
    pub const V: usize = offset_of!(UpVal, v);
}

/// `LClosure` (Lua closure) fields.
pub mod lclosure {
    use super::*;
    /// Pointer to the `Proto`.
    pub const PROTO: usize = offset_of!(LClosure, p);
    /// Offset of the first upvalue pointer (the array follows the struct).
    pub const UPVALS: usize = size_of::<LClosure>();
}

/// `CClosure` (native closure) fields (BUNDLE-T1I: tier-1 inline `type`).
pub mod cclosure {
    use super::*;
    use crate::func::CClosure;
    /// `nupvalues` (byte).
    pub const NUPVALUES: usize = offset_of!(CClosure, nupvalues);
    /// The native function pointer `f`.
    pub const F: usize = offset_of!(CClosure, f);
    /// Offset of the first upvalue (the array follows the struct).
    pub const UPVALS: usize = size_of::<CClosure>();
}

/// `Proto` fields. The `k`/`code` buffers are `RawBuf`s whose internal order is not fixed: use
/// [`proto_layout`] with a live `Proto` to get the offsets of their data pointers.
pub mod proto {
    use super::*;
    /// `numparams` (byte).
    pub const NUMPARAMS: usize = offset_of!(Proto, numparams);
    /// `maxstacksize` (byte).
    pub const MAXSTACKSIZE: usize = offset_of!(Proto, maxstacksize);
    /// Offset of the `k` buffer; its data pointer is `K_BUF + `[`ProtoLayout::k_ptr_in_buf`].
    pub const K_BUF: usize = offset_of!(Proto, k);
    /// Offset of the `code` buffer.
    pub const CODE_BUF: usize = offset_of!(Proto, code);
    /// Size of one `RawBuf` (data pointer + capacity).
    pub const BUF_SIZE: usize = size_of::<crate::alloc::RawBuf<u32>>();
    /// A pointer-sized field that is only used by the collector's gray lists (`gclist`). Prototype code that
    /// needs to emulate "`Proto.jit`" while the collector is stopped may use it as a scratch slot (the JIT
    /// oracle does; the real design appends a field instead).
    pub const GCLIST: usize = offset_of!(Proto, gclist);
    /// `Proto.jit` (compiled-code descriptor pointer; only with the `jit` feature).
    #[cfg(feature = "jit")]
    pub const JIT: usize = offset_of!(Proto, jit);
}

/// Offsets of the data pointers inside a `Proto`, resolved from a live object (see [`proto_layout`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtoLayout {
    /// Byte offset of the code (`*const u32`) data pointer from the start of the `Proto`.
    pub code_ptr: usize,
    /// Byte offset of the constants (`*const Value`) data pointer.
    pub k_ptr: usize,
}

/// Locates the `code` and `k` data pointers inside `p` by searching the `RawBuf`s for the addresses reported
/// by `Proto::code_ptr` / `Proto::k_ptr` (the buffer's internal field order is a Rust layout detail).
/// Returns `None` if a buffer is empty (null data pointer) or the address is not found.
pub fn proto_layout(p: &Proto) -> Option<ProtoLayout> {
    fn find(p: &Proto, buf_off: usize, want: usize) -> Option<usize> {
        if want == 0 {
            return None;
        }
        let base = (p as *const Proto).cast::<u8>();
        (0..proto::BUF_SIZE)
            .step_by(size_of::<usize>())
            .map(|d| buf_off + d)
            // SAFETY: the offset is inside the `Proto` (the buffer is one of its fields); the read is
            // unaligned-safe and only inspects bytes.
            .find(|&o| unsafe { base.add(o).cast::<usize>().read_unaligned() } == want)
    }
    Some(ProtoLayout {
        code_ptr: find(p, proto::CODE_BUF, p.code_ptr() as usize)?,
        k_ptr: find(p, proto::K_BUF, p.k_ptr() as usize)?,
    })
}

/// [`proto_layout`] for the `Proto` type, resolved once from a throw-away prototype (the offsets are a
/// property of the type, but a given `Proto` may have an empty constants/code buffer).
pub fn proto_layout_probe() -> ProtoLayout {
    use crate::protodata::{Const, ProtoData};
    static CACHE: std::sync::OnceLock<ProtoLayout> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        let mut h = crate::heap::Heap::new(1).expect("heap");
        let pd: ProtoData<crate::string::StrRef> = ProtoData {
            num_params: 0,
            flags: 0,
            max_stack_size: 2,
            line_defined: 0,
            last_line_defined: 0,
            code: vec![0],
            k: vec![Const::Int(1)],
            protos: vec![],
            upvalues: vec![],
            line_info: vec![],
            abs_line_info: vec![],
            loc_vars: vec![],
            source: None,
        };
        let p = Proto::from_data(&mut h, &pd).expect("proto");
        proto_layout(&p).expect("Proto layout probe")
    })
}

/// `CallInfo`.
pub mod callinfo {
    use super::*;
    /// `size_of::<CallInfo>()`.
    pub const SIZE: usize = size_of::<CallInfo>();
    /// `func`: stack *index* (`usize`) of the function slot.
    pub const FUNC: usize = offset_of!(CallInfo, func);
    /// `top`: stack index.
    pub const TOP: usize = offset_of!(CallInfo, top);
    /// `previous` pointer.
    pub const PREVIOUS: usize = offset_of!(CallInfo, previous);
    /// `next` pointer (null = no spare node).
    pub const NEXT: usize = offset_of!(CallInfo, next);
    /// `savedpc` (`*const u32`).
    pub const SAVEDPC: usize = offset_of!(CallInfo, savedpc);
    /// `trap` (`i32`).
    pub const TRAP: usize = offset_of!(CallInfo, trap);
    /// `nextraargs` (`i32`).
    pub const NEXTRAARGS: usize = offset_of!(CallInfo, nextraargs);
    /// `kcache` (`*const Value`).
    pub const KCACHE: usize = offset_of!(CallInfo, kcache);
    /// `callstatus` (`u32`): bits 0-7 hold the expected number of results plus one.
    pub const CALLSTATUS: usize = offset_of!(CallInfo, callstatus);
    /// Mask of the expected-results field of `callstatus`.
    pub const NRESULTS_MASK: u32 = crate::state::CIST_NRESULTS;
    /// `CIST_C`: the frame belongs to a native function.
    pub const CIST_C: u32 = crate::state::CIST_C;
    /// `CIST_FRESH`: the frame is the entry of a `run` invocation (returning ends the interpreter loop).
    pub const CIST_FRESH: u32 = crate::state::CIST_FRESH;
    /// `CIST_MMRET`: a metamethod run in place; returning must finish the caller's instruction.
    pub const CIST_MMRET: u32 = crate::state::CIST_MMRET;
    /// `CIST_TAIL`: the frame was reused by a tail call.
    pub const CIST_TAIL: u32 = crate::state::CIST_TAIL;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::Heap;
    use crate::value::*;

    /// Inline GC barriers (templates_table) read the `marked` byte of an arbitrary collectable value
    /// at the table's offset: it must be the same in every object header.
    #[test]
    fn marked_offset_is_shared() {
        use crate::func::CClosure;
        use crate::state::Thread;
        use crate::udata::Udata;
        let m = table::MARKED;
        assert_eq!(m, offset_of!(GcHeader, marked));
        assert_eq!(m, offset_of!(LuaString, hdr) + offset_of!(GcHeader, marked));
        assert_eq!(m, offset_of!(UpVal, hdr) + offset_of!(GcHeader, marked));
        assert_eq!(m, offset_of!(LClosure, marked));
        assert_eq!(m, offset_of!(CClosure, marked));
        assert_eq!(m, offset_of!(Udata, hdr) + offset_of!(GcHeader, marked));
        assert_eq!(m, offset_of!(Thread, hdr) + offset_of!(GcHeader, marked));
    }

    #[test]
    fn sizes() {
        assert_eq!(value::SIZE, 16);
        assert_eq!(size_of::<Table>(), 48);
        assert_eq!(table::SIZE, 48);
        assert_eq!(node::SIZE, 24);
        assert_eq!(size_of::<GcHeader>(), 16);
    }

    #[test]
    fn repr_c_expectations() {
        assert_eq!(value::TAG, 0);
        assert_eq!(value::PAYLOAD, 8);
        assert_eq!(table::MARKED, 9);
        assert_eq!(table::FLAGS, 10);
        assert_eq!(table::LSIZENODE, 11);
        assert_eq!(table::ASIZE, 12);
        assert_eq!(table::ARRAY, 16);
        assert_eq!(table::NODE, 24);
        assert_eq!(table::METATABLE, 32);
        assert_eq!(node::VAL, 0);
        assert_eq!(node::VAL_TT, 8);
        assert_eq!(node::KEY_TT, 9);
        assert_eq!(node::NEXT, 12);
        assert_eq!(node::KEY, 16);
        assert_eq!(string::SHORT_HASH, 12);
        assert_eq!(lclosure::PROTO, 24);
        assert_eq!(upval::V, offset_of!(UpVal, v));
        assert_eq!(callinfo::FUNC, 0);
        assert_eq!(callinfo::TOP, 8);
        assert_eq!(callinfo::PREVIOUS, 16);
        assert_eq!(callinfo::NEXT, 24);
        assert_eq!(callinfo::SAVEDPC, 32);
        assert_eq!(callinfo::TRAP, 40);
        assert_eq!(callinfo::NEXTRAARGS, 44);
        assert_eq!(callinfo::KCACHE, 48);
        const { assert!(callinfo::CALLSTATUS + 4 <= callinfo::SIZE) };
    }

    /// Reads a `T` at byte offset `off` from `p`.
    ///
    /// # Safety
    /// `p + off` must be a readable, initialised `T`.
    unsafe fn rd<T: Copy>(p: *const u8, off: usize) -> T {
        // SAFETY: per contract.
        unsafe { p.add(off).cast::<T>().read_unaligned() }
    }

    #[test]
    fn table_array_and_node_offsets_match_a_real_table() {
        let mut h = Heap::new(7).unwrap();
        let t = h.new_table().unwrap();
        t.resize(&mut h, 4, 4).unwrap();
        for i in 1..=4 {
            t.set_int(&mut h, i, &Value::int(100 + i)).unwrap();
        }
        t.set_int(&mut h, 2, &Value::float(2.5)).unwrap();
        let key = h.new_str(b"field").unwrap();
        t.set_str(&mut h, key, &Value::int(77)).unwrap();
        let p = t.as_ptr().cast::<u8>().cast_const();
        // SAFETY: `p` is a live table; every offset below is inside it or inside its array / node blocks.
        unsafe {
            assert_eq!(rd::<u32>(p, table::ASIZE), 4);
            let arr: *const u8 = rd(p, table::ARRAY);
            for k in 0..4usize {
                let tag: u8 = rd(arr, table::ARRAY_TAG_BASE + k);
                // (the payloads lie BELOW the array pointer: `sub`, not `add` of a wrapped offset, which is a pointer-arithmetic overflow)
                let pay: u64 = arr
                    .sub(table::ARRAY_PAYLOAD_STRIDE * (k + 1))
                    .cast::<u64>()
                    .read_unaligned();
                let want = t.get_int(k as i64 + 1);
                assert_eq!(tag, want.tag());
                if k == 1 {
                    assert_eq!(f64::from_bits(pay), 2.5);
                } else {
                    assert_eq!(pay as i64, 101 + k as i64);
                }
            }
            // hash part: find the node of "field" by scanning the node array
            let node0: *const u8 = rd(p, table::NODE);
            let n = 1usize << rd::<u8>(p, table::LSIZENODE);
            let mut found = false;
            for i in 0..n {
                let nd = node0.add(i * node::SIZE);
                if rd::<u8>(nd, node::KEY_TT) == ctb(LUA_VSHRSTR)
                    && rd::<usize>(nd, node::KEY) == key.as_ptr() as usize
                {
                    assert_eq!(rd::<u8>(nd, node::VAL_TT), LUA_VNUMINT);
                    assert_eq!(rd::<i64>(nd, node::VAL), 77);
                    found = true;
                }
            }
            assert!(found, "node not found at the derived offsets");
            // the key's main position follows hash & (size - 1)
            let hash: u32 = rd(key.as_ptr().cast_const().cast::<u8>(), string::SHORT_HASH);
            assert_eq!(hash, LuaString::short_hash(key.as_ptr()));
            assert!(rd::<*const u8>(p, table::METATABLE).is_null());
        }
    }

    #[test]
    fn value_offsets_match_a_real_value() {
        let v = Value::float(1.5);
        let p = (&v as *const Value).cast::<u8>();
        // SAFETY: reads inside `v`.
        unsafe {
            assert_eq!(rd::<u64>(p, value::TAG), u64::from(LUA_VNUMFLT));
            assert_eq!(f64::from_bits(rd::<u64>(p, value::PAYLOAD)), 1.5);
        }
    }

    #[test]
    fn proto_layout_finds_the_data_pointers() {
        use crate::protodata::{Const, ProtoData};
        let mut h = Heap::new(3).unwrap();
        let pd: ProtoData<crate::string::StrRef> = ProtoData {
            num_params: 2,
            flags: 0,
            max_stack_size: 5,
            line_defined: 0,
            last_line_defined: 0,
            code: vec![1, 2, 3],
            k: vec![Const::Int(9), Const::Int(10)],
            protos: vec![],
            upvalues: vec![],
            line_info: vec![],
            abs_line_info: vec![],
            loc_vars: vec![],
            source: None,
        };
        let g = Proto::from_data(&mut h, &pd).unwrap();
        let p: &Proto = &g;
        assert_eq!(p.numparams, 2);
        assert_eq!(p.maxstacksize, 5);
        let l = proto_layout(p).expect("layout");
        let base = (p as *const Proto).cast::<u8>();
        // SAFETY: `base` is the proto; the scalar offsets are fields of it.
        unsafe {
            assert_eq!(rd::<u8>(base, proto::NUMPARAMS), 2);
            assert_eq!(rd::<u8>(base, proto::MAXSTACKSIZE), 5);
        }
        // SAFETY: offsets come from the search over the proto's own bytes.
        unsafe {
            assert_eq!(rd::<usize>(base, l.code_ptr), p.code_ptr() as usize);
            assert_eq!(rd::<usize>(base, l.k_ptr), p.k_ptr() as usize);
        }
        assert_eq!(proto::BUF_SIZE, 16);
    }

    #[test]
    fn version_constants() {
        assert_eq!(LUA54, cfg!(rustynail_lua54));
        assert_eq!(table::BITDUMMY, 1 << 6);
        assert_eq!(callinfo::NRESULTS_MASK, 0xff);
    }
}
