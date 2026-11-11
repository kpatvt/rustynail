//! The interpreter exports the C API (`lua_*`, `luaL_*`) so that C modules loaded with
//! `package.loadlib`/`require` resolve their symbols from the host executable.
fn main() {
    println!("cargo:rustc-link-arg-bins=-rdynamic");
}
