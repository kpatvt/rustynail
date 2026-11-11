//! The interface the compiler/undump use to talk to the runtime (docs/ARCH.md §Contracts).
//! Builder A develops against a test double; Builder B implements it for the real `State`.
use crate::status::LuaResult;

/// Services the front end needs from whoever owns the heap.
pub trait CompilerHost {
    /// Interned-string handle. Cheap to copy; equality == byte equality for the lifetime of a compile.
    type Str: Copy + Eq + std::fmt::Debug;

    /// Intern `bytes` (short strings interned, long strings may be fresh objects). Fails with `ErrMem`.
    fn intern(&mut self, bytes: &[u8]) -> LuaResult<Self::Str>;

    /// Bytes of a string created by this host.
    fn bytes(&self, s: Self::Str) -> &[u8];

    /// Object identity of `s` (the 5.4 parser's `eqstr`, a pointer comparison). Two strings with
    /// equal bytes have equal ids when both are short (interned) or both were created by the same
    /// lexer (it reuses one object per long-string content, as `luaX_newstring` does).
    fn str_id(s: Self::Str) -> usize;
}
