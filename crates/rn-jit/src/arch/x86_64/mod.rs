//! x86-64 instruction encoder (safe code, bytes in a `Vec<u8>`), with labels and fixups.
//!
//! See [`Assembler`]. Everything here is plain safe Rust; executing the produced bytes is the business
//! of the `mem` module. Instructions needing more than baseline x86-64 (SSE2) are documented on the
//! method (only `roundsd`, SSE4.1).

mod asm;
mod regs;

#[cfg(test)]
mod exec_tests;
#[cfg(test)]
mod tests;

pub use asm::{AsmError, Assembler, Label, Mem};
pub use regs::*;
