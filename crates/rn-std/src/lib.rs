//! rn-std — the standard libraries. The State-independent engines (`pattern`, `format`, `pack`,
//! `utf8`, `random`, `strtime`, `cfmt`) are pure functions over `&[u8]`/numbers; the `lib_*` modules
//! are the VM-facing glue (`luaopen_*`, argument checking via `rn_core::auxlib`).
//! See docs/ARCH.md.

pub mod buf_store;
pub mod cfmt;
pub mod dl_ffi;
pub mod format;
pub mod io_ffi;
pub mod pack;
pub mod pattern;
pub mod random;
pub mod strfast;
pub mod strtime;
pub mod utf8;

pub mod lib_base;
pub mod lib_buffer;
pub mod lib_coroutine;
pub mod lib_debug;
pub mod lib_ext;
pub mod lib_io;
pub mod lib_math;
pub mod lib_os;
pub mod lib_package;
pub mod lib_string;
pub mod lib_table;
pub mod lib_utf8;

use rn_core::heap::Heap;
use rn_core::protodata::ProtoData;
use rn_core::state::{LuaState, NativeFn, State};
use rn_core::status::LuaResult;
use rn_core::string::StrRef;

/// The chunk loader installed into every state: the front end with the raw `load` mode string.
pub fn loader(
    heap: &mut Heap,
    chunk: &[u8],
    chunk_name: &[u8],
    mode: &[u8],
) -> Result<ProtoData<StrRef>, Vec<u8>> {
    rn_compiler::load_with_mode_str(heap, chunk, chunk_name, Some(mode)).map_err(|e| e.message)
}

/// The library list of `luaL_openlibs`, in the reference order. (`coroutine` is registered by the
/// coroutine task through `EXTRA_LIBS` hooks in the facade.)
pub const STD_LIBS: &[(&[u8], NativeFn)] = &[
    (b"_G", lib_base::open),
    (b"package", lib_package::open),
    (b"coroutine", lib_coroutine::open),
    (b"table", lib_table::open),
    (b"io", lib_io::open),
    (b"os", lib_os::open),
    (b"string", lib_string::open),
    (b"math", lib_math::open),
    (b"utf8", lib_utf8::open),
    (b"debug", lib_debug::open),
];

/// `luaL_openlibs`.
pub fn open_libs(st: &mut State) -> LuaResult<()> {
    for (name, f) in STD_LIBS {
        st.requiref(name, *f, true)?;
        st.pop();
    }
    Ok(())
}

/// A fresh state with the loader and the warning function installed (no libraries opened).
pub fn new_state(seed: u32) -> LuaResult<LuaState> {
    let mut st = State::with_seed(seed)?;
    st.set_loader(loader);
    Ok(st)
}

/// A fresh state with all standard libraries open (`luaL_newstate` + `luaL_openlibs`).
pub fn new_state_with_libs() -> LuaResult<LuaState> {
    let mut st = new_state(rn_core::auxlib::make_seed())?;
    open_libs(&mut st)?;
    Ok(st)
}
