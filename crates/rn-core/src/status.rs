//! Internal error status (see docs/ARCH.md §Errors). Lua error *values* live in the State, not here.
/// Mirrors the reference `LUA_*` thread/error status codes (`lua.h`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok = 0,
    Yield = 1,
    ErrRun = 2,
    ErrSyntax = 3,
    ErrMem = 4,
    ErrErr = 5,
}

/// Result type used throughout the VM. 1-byte error so it stays in registers.
pub type LuaResult<T> = Result<T, Status>;
