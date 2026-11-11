//! v55: version-specific tables and helpers.
pub mod opcodes_gen_mod {
    include!("v55/opcodes_gen.rs");
}
pub use opcodes_gen_mod::*;
