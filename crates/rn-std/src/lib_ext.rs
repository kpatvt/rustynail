//! LuaJIT-compatible extensions (docs/EXTENSIONS.md): `table.new`, `table.clear`, the `string.buffer` preload and the
//! `jit` control table. Opened separately from the standard libraries ([`open_ext`], [`open_jit`]); `rnlua` opens both.
#![forbid(unsafe_code)]

use rn_core::auxlib::{LUA_LOADED_TABLE, LUA_PRELOAD_TABLE};
use rn_core::state::{NativeFn, State};
use rn_core::status::LuaResult;
use rn_core::value::Value;
use std::sync::OnceLock;

/// Largest array / hash size `table.new` accepts (LuaJIT's limits; larger requests are "table overflow").
const MAX_NEW_ARRAY: f64 = 268435456.0; // 2^28
const MAX_NEW_HASH: f64 = 67108864.0; // 2^26

/// A size argument of `table.new`: a number, truncated; negative, NaN or too large is "table overflow".
fn size_arg(st: &mut State, n: usize, max: f64) -> LuaResult<usize> {
    let x = st.check_number(n)?;
    let x = x.trunc();
    if x.is_nan() || !(0.0..=max).contains(&x) {
        return Err(st.l_error(b"table overflow"));
    }
    Ok(x as usize)
}

/// `table.new(narray, nhash)`: a table with the given array and hash capacity.
fn tnew(st: &mut State) -> LuaResult<usize> {
    let narr = size_arg(st, 1, MAX_NEW_ARRAY)?;
    let nhash = size_arg(st, 2, MAX_NEW_HASH)?;
    let t = st.new_table_sized(narr, nhash)?;
    st.push(Value::table(t));
    Ok(1)
}

/// `table.clear(t)`: removes all entries (raw), keeps the capacity and the metatable; returns nothing.
fn tclear(st: &mut State) -> LuaResult<usize> {
    let t = st.check_table(1)?;
    t.clear_values();
    Ok(0)
}

fn load_new(st: &mut State) -> LuaResult<usize> {
    st.push(Value::native(tnew));
    Ok(1)
}

fn load_clear(st: &mut State) -> LuaResult<usize> {
    st.push(Value::native(tclear));
    Ok(1)
}

/// Adds `table.new` / `table.clear` (when the `table` library is open) and registers the loaders
/// `package.preload["table.new" | "table.clear" | "string.buffer"]`. `string.buffer` is deliberately not a field of
/// `string` (LuaJIT only offers it through `require`).
pub fn open_ext(st: &mut State) -> LuaResult<()> {
    let reg = st.registry();
    let (loaded, _) = st.get_sub_table(reg, LUA_LOADED_TABLE)?;
    st.push(Value::table(loaded));
    if let Some(tbl) = st.raw_get_str(loaded, b"table")?.as_table() {
        st.raw_set_str(tbl, b"new", Value::native(tnew))?;
        st.raw_set_str(tbl, b"clear", Value::native(tclear))?;
    }
    let (pre, _) = st.get_sub_table(reg, LUA_PRELOAD_TABLE)?;
    st.push(Value::table(pre));
    for (name, f) in [
        (&b"table.new"[..], load_new as NativeFn),
        (b"table.clear", load_clear),
        (b"string.buffer", crate::lib_buffer::open),
    ] {
        st.raw_set_str(pre, name, Value::native(f))?;
    }
    st.pop();
    st.pop();
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// the `jit` table
// ---------------------------------------------------------------------------------------------

/// The compiler's side of the `jit` table, installed by `rn_jit::register` (rn-std does not depend on rn-jit).
#[allow(clippy::type_complexity)]
pub struct JitCtl {
    /// `Some((enabled, features))` when a compiler is installed for this state, `None` otherwise.
    pub status: fn(&mut State) -> Option<(bool, Vec<&'static str>)>,
    /// `jit.on()` (`true`) / `jit.off()` (`false`): the global switch for this state.
    pub set_enabled: fn(&mut State, bool),
    /// `jit.flush()`: drops the compiled traces of this state.
    pub flush: fn(&mut State),
}

static JIT_CTL: OnceLock<JitCtl> = OnceLock::new();

/// Installs the compiler's control hooks (first call wins).
pub fn set_jit_ctl(c: JitCtl) {
    let _ = JIT_CTL.set(c);
}

fn jit_status(st: &mut State) -> LuaResult<usize> {
    match JIT_CTL.get().and_then(|c| (c.status)(st)) {
        Some((enabled, features)) => {
            st.check_stack(features.len() + 2)?;
            st.push(Value::boolean(enabled));
            for f in &features {
                st.push_str(f)?;
            }
            Ok(1 + features.len())
        }
        None => {
            st.push(Value::FALSE);
            Ok(1)
        }
    }
}

/// `jit.on()` / `jit.off()`: the global switch. The per-function forms (`jit.on(f)`, `jit.off(f, true)`) are accepted
/// and ignored.
fn jit_switch(st: &mut State, on: bool) -> LuaResult<usize> {
    if st.arg_is_none_or_nil(1) {
        if let Some(c) = JIT_CTL.get() {
            (c.set_enabled)(st, on);
        }
    }
    Ok(0)
}

fn jit_on(st: &mut State) -> LuaResult<usize> {
    jit_switch(st, true)
}

fn jit_off(st: &mut State) -> LuaResult<usize> {
    jit_switch(st, false)
}

fn jit_flush(st: &mut State) -> LuaResult<usize> {
    if let Some(c) = JIT_CTL.get() {
        (c.flush)(st);
    }
    Ok(0)
}

fn version_num() -> i64 {
    let mut it = env!("CARGO_PKG_VERSION")
        .split('.')
        .map(|x| x.parse::<i64>().unwrap_or(0));
    let (a, b, c) = (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    );
    a * 10000 + b * 100 + c
}

/// Opens the global `jit` table (also `package.loaded.jit`): `version`, `version_num`, `os`, `arch`, `on`, `off`,
/// `flush`, `status`.
pub fn open_jit(st: &mut State) -> LuaResult<()> {
    let t = st.new_table_sized(0, 8)?;
    st.push(Value::table(t));
    st.set_funcs(
        t,
        &[
            ("on", Some(jit_on)),
            ("off", Some(jit_off)),
            ("flush", Some(jit_flush)),
            ("status", Some(jit_status)),
        ],
    )?;
    let v = st.new_str_value(format!("rustynail {} JIT", env!("CARGO_PKG_VERSION")).as_bytes())?;
    st.raw_set_str(t, b"version", v)?;
    st.raw_set_str(t, b"version_num", Value::int(version_num()))?;
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "macos" => "OSX",
        "windows" => "Windows",
        "freebsd" | "netbsd" | "openbsd" | "dragonfly" => "BSD",
        _ => "Other",
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "arm64",
        "arm" => "arm",
        other => other,
    };
    let v = st.new_str_value(os.as_bytes())?;
    st.raw_set_str(t, b"os", v)?;
    let v = st.new_str_value(arch.as_bytes())?;
    st.raw_set_str(t, b"arch", v)?;
    let g = st.globals();
    st.raw_set_str(g, b"jit", Value::table(t))?;
    let reg = st.registry();
    let (loaded, _) = st.get_sub_table(reg, LUA_LOADED_TABLE)?;
    st.push(Value::table(loaded));
    st.raw_set_str(loaded, b"jit", Value::table(t))?;
    st.pop();
    st.pop();
    Ok(())
}
