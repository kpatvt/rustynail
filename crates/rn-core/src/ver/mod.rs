//! Version-specific items. Exactly one of `rustynail_lua54` / `rustynail_lua55` is active
//! (5.5 is the default when neither cfg is passed; see `cargo xtask`).
#[cfg(all(rustynail_lua54, rustynail_lua55))]
compile_error!("rustynail_lua54 and rustynail_lua55 are mutually exclusive");

#[cfg(rustynail_lua54)]
pub mod v54;
#[cfg(not(rustynail_lua54))]
pub mod v55;

#[cfg(rustynail_lua54)]
pub use v54 as active;
#[cfg(not(rustynail_lua54))]
pub use v55 as active;

/// Human-readable active version, e.g. for `_VERSION`.
#[cfg(rustynail_lua54)]
pub const LUA_VERSION: &str = "Lua 5.4";
#[cfg(not(rustynail_lua54))]
pub const LUA_VERSION: &str = "Lua 5.5";
