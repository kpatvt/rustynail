//! Compiles the C shim for the C-variadic entry points (`lua_pushfstring`, `luaL_error`,
//! `lua_gc`): stable Rust cannot define C-variadic functions. The shim only extracts the
//! arguments (`va_arg`) and hands them to Rust (`csrc/shim.c`).
fn main() {
    println!("cargo:rerun-if-changed=csrc/shim.c");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_RUSTYNAIL_LUA54");
    let mut b = cc::Build::new();
    if std::env::var_os("CARGO_CFG_RUSTYNAIL_LUA54").is_some() {
        b.define("RN_LUA54", None);
    }
    b.file("csrc/shim.c")
        .warnings(true)
        .flag_if_supported("-fexceptions")
        .compile("rn_capi_shim");
}
