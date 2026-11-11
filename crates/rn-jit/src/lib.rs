//! Experimental x86-64 JIT backend (Phase 10, see `docs/JIT_PLAN.md`).
//!
//! Nothing in this crate is used by the interpreter unless the `jit` feature is enabled (plan F1). It is
//! organised as: `arch::x86_64` (instruction encoder, no knowledge of Lua), `mem` (executable memory
//! manager: W^X arenas, probe, code cap), and, with the `jit` feature, the template compiler: `ir` (micro-op
//! IR), `emu` (Rust emulator of the IR, runnable under Miri), `x86` (IR backend and entry thunk), `lower`
//! (bytecode -> IR templates) and `rt` (runtime: configuration, per-state arena, entry, registration).
//!
//! All `unsafe` in this crate is confined to `mem` (executable memory), `rt` (calling generated code) and
//! the emulator's raw memory accesses; the encoder is safe code operating on `Vec<u8>`.
// Internal-only crate (nothing is published, the facade re-exports a small surface): the JIT's constants, phase counters and
// IR fields are documented where they are defined or by their names, and the unsafe blocks of the trace compiler share the
// crate-level contract above (as in rn-capi).
#![allow(missing_docs, clippy::undocumented_unsafe_blocks)]

pub mod arch;
#[cfg(feature = "jit")]
#[doc(hidden)]
pub mod fx;
pub mod mem;

#[cfg(feature = "jit")]
pub mod emu;
#[cfg(feature = "jit")]
pub mod ir;
#[cfg(feature = "jit")]
pub mod lower;
#[cfg(feature = "jit")]
pub mod rt;
#[cfg(feature = "jit")]
pub mod t15;
#[cfg(feature = "jit")]
mod templates_alloc;
#[cfg(feature = "jit")]
mod templates_arith;
#[cfg(feature = "jit")]
mod templates_loops;
#[cfg(feature = "jit")]
mod templates_table;
#[cfg(feature = "trace")]
pub mod tr;
#[cfg(feature = "jit")]
pub mod x86;

#[cfg(feature = "jit")]
pub use rt::{
    available, register, register_with, register_with_trace_pinned_off, set_code_cap,
    set_count_budget, set_mode, set_t15, stats, Config, Mode, Stats,
};
#[cfg(feature = "trace")]
pub use rt::{set_trace, set_trace_enabled};
