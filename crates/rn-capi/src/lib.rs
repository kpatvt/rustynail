//! `rn-capi`: the Lua 5.5 (or, under `--cfg rustynail_lua54`, 5.4) C API (`lua.h`, `lauxlib.h`, `lualib.h`) on top of `rn-core`.
//!
//! The index-based `lua_*` functions are a thin layer over the internal `State` API; the stdlib
//! itself never goes through them (PLAN §4.5). Errors cross C frames as Rust panics carrying a
//! private `glue::Throw` payload (`resume_unwind`, "C-unwind" ABI); they are caught by the
//! trampolines that call C functions (`glue::catch`), which turn them back into `Err(Status)`.
//!
//! # Safety contract of every exported function
//! `L` must be a valid `lua_State*` obtained from this library, indices must be acceptable
//! (`lua_checkstack` was used to reserve space), pointers must be valid C pointers; exactly the
//! contract of the reference API. The functions are `unsafe extern "C-unwind"` and are documented
//! once here instead of on each item.
#![allow(
    clashing_extern_declarations, // `keep.rs` declares the API functions by name only
    non_camel_case_types,
    non_snake_case,
    clippy::missing_safety_doc,
    clippy::undocumented_unsafe_blocks,
    clippy::not_unsafe_ptr_arg_deref
)]

#[macro_use]
mod glue;
pub mod fmt;
pub mod keep;
pub mod lapi;
pub mod lapi_call;
pub mod lapi_debug;
pub mod lapi_get;
pub mod lauxlib;
pub mod lauxlib_buf;
pub mod lualib;
pub mod types;

pub use glue::ensure_init;
pub use keep::exported_symbols;
pub use types::*;
