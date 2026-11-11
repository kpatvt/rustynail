//! Plain-Rust function prototype produced by the compiler / undump and consumed by the VM.
//!
//! This is the **A→B contract** (docs/ARCH.md §Contracts). `S` is a host-provided interned-string
//! handle (see [`crate::host::CompilerHost`]). Fields mirror `Proto` in the vendored `lobject.h`
//! for the active Lua version; extend here (orchestrator-approved) if a version needs more.

/// A constant-table entry (`K`).
#[derive(Clone, Debug, PartialEq)]
pub enum Const<S> {
    Nil,
    False,
    True,
    Int(i64),
    Float(f64),
    Str(S),
}

/// Upvalue descriptor (`Upvaldesc`).
#[derive(Clone, Debug, PartialEq)]
pub struct UpvalDesc<S> {
    pub name: Option<S>,
    pub in_stack: bool,
    pub idx: u8,
    /// `kind`: regular / const / compile-time-const (`VDKREG`, `RDKCONST`, `RDKTOCLOSE`, `RDKCTC`).
    pub kind: u8,
}

/// Local variable debug info (`LocVar`).
#[derive(Clone, Debug, PartialEq)]
pub struct LocVar<S> {
    pub name: S,
    pub start_pc: u32,
    pub end_pc: u32,
}

/// Absolute line info entry (`AbsLineInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbsLineInfo {
    pub pc: u32,
    pub line: i32,
}

/// Function has hidden vararg arguments (`...`).
pub const PF_VAHID: u8 = 1;
/// Function has a vararg table (5.5 named varargs `...t`).
pub const PF_VATAB: u8 = 2;

/// A compiled function.
#[derive(Clone, Debug, PartialEq)]
pub struct ProtoData<S> {
    pub num_params: u8,
    /// Raw `Proto.flag` bits: [`PF_VAHID`] | [`PF_VATAB`] (the reference's `PF_FIXED` is runtime-only, never stored).
    /// 5.4 only ever uses `PF_VAHID` (1) for vararg functions.
    pub flags: u8,
    pub max_stack_size: u8,
    pub line_defined: i32,
    pub last_line_defined: i32,
    pub code: Vec<u32>,
    pub k: Vec<Const<S>>,
    pub protos: Vec<ProtoData<S>>,
    pub upvalues: Vec<UpvalDesc<S>>,
    pub line_info: Vec<i8>,
    pub abs_line_info: Vec<AbsLineInfo>,
    pub loc_vars: Vec<LocVar<S>>,
    pub source: Option<S>,
}

impl<S> ProtoData<S> {
    /// True if the function is vararg (`isvararg` in the reference).
    pub fn is_vararg(&self) -> bool {
        self.flags & (PF_VAHID | PF_VATAB) != 0
    }
}
