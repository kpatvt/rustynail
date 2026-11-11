//! rn-core — runtime: values, strings, tables, GC, State, interpreter. See docs/ARCH.md.
pub mod alloc;
pub mod arith;
pub mod auxlib;
pub mod call;
pub mod collect;
pub mod coro;
pub mod debuginfo;
pub mod error;
pub mod func;
pub mod gc;
pub mod frameless;
pub mod heap;
pub mod host;
pub mod instruction;
#[cfg(feature = "jit")]
pub mod jit_helpers;
#[cfg(feature = "jit")]
pub mod jit_hooks;
pub mod jit_layout;
pub mod meta;
pub mod nfmt;
pub mod number;
#[cfg(feature = "t15_oracle")]
pub mod oracle;
pub mod protodata;
pub mod state;
pub mod status;
pub mod string;
pub mod table;
pub mod udata;
pub mod value;
pub mod ver;
pub mod vm;
mod vm_unsafe;

#[cfg(test)]
mod tests_gc;
#[cfg(test)]
mod tests_state;
#[cfg(test)]
mod tests_table;
