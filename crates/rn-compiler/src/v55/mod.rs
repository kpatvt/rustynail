//! Lua 5.5 parser and code generator (ports of `lparser.c` / `lcode.c`).
//!
//! The C code threads a `FuncState` chain through `prev` pointers; here the active functions live in
//! `Parser::fstack` and parser helpers take a function index where the C code takes an arbitrary `fs`.
//! Code-generation helpers (`k_*`, from `lcode.c`) always act on the innermost function.
use crate::lexer::{LexState, Token};
use crate::CompileError;
use rn_core::host::CompilerHost;
use rn_core::protodata::{Const, ProtoData};

mod codegen;
mod parser;

pub use parser::compile;

pub(crate) type R<T> = Result<T, CompileError>;

pub(crate) const NO_JUMP: i32 = -1;
pub(crate) const MAXVARS: i32 = 200;
pub(crate) const MAXUPVAL: i32 = 255;
pub(crate) const MAX_FSTACK: i32 = 255;
pub(crate) const NO_REG: i32 = MAX_FSTACK;
pub(crate) const MAXINDEXRK: i32 = 255;
pub(crate) const LUA_MULTRET: i32 = -1;
pub(crate) const MAXIWTHABS: u8 = 128;
pub(crate) const LUAI_MAXCCALLS: u32 = 200;
pub(crate) const SHRT_MAX: usize = 32767;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExpKind {
    Void,
    Nil,
    True,
    False,
    K,
    KFlt,
    KInt,
    KStr,
    NonReloc,
    Local,
    VarGVar,
    Global,
    Upval,
    Const,
    Indexed,
    VarGInd,
    IndexUp,
    IndexI,
    IndexStr,
    Jmp,
    Reloc,
    Call,
    Vararg,
}

impl ExpKind {
    pub(crate) fn is_var(self) -> bool {
        use ExpKind::*;
        matches!(
            self,
            Local
                | VarGVar
                | Global
                | Upval
                | Const
                | Indexed
                | VarGInd
                | IndexUp
                | IndexI
                | IndexStr
        )
    }
    pub(crate) fn is_indexed(self) -> bool {
        use ExpKind::*;
        matches!(self, Indexed | VarGInd | IndexUp | IndexI | IndexStr)
    }
    pub(crate) fn has_multret(self) -> bool {
        matches!(self, ExpKind::Call | ExpKind::Vararg)
    }
}

/// Expression descriptor. The C union is flattened into separate fields.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ExpDesc<S: Copy> {
    pub k: ExpKind,
    pub info: i32,
    pub ival: i64,
    pub nval: f64,
    pub strval: Option<S>,
    pub ind_idx: i32,
    pub ind_t: i32,
    pub ind_ro: bool,
    pub ind_keystr: i32,
    pub var_ridx: i32,
    pub var_vidx: i32,
    pub t: i32,
    pub f: i32,
}

impl<S: Copy> ExpDesc<S> {
    pub(crate) fn new(k: ExpKind, info: i32) -> Self {
        ExpDesc {
            k,
            info,
            ival: 0,
            nval: 0.0,
            strval: None,
            ind_idx: 0,
            ind_t: 0,
            ind_ro: false,
            ind_keystr: -1,
            var_ridx: 0,
            var_vidx: 0,
            t: NO_JUMP,
            f: NO_JUMP,
        }
    }
    pub(crate) fn has_jumps(&self) -> bool {
        self.t != self.f
    }
}

// kinds of variables
pub(crate) const VDKREG: u8 = 0;
pub(crate) const RDKCONST: u8 = 1;
pub(crate) const RDKVAVAR: u8 = 2;
pub(crate) const RDKTOCLOSE: u8 = 3;
pub(crate) const RDKCTC: u8 = 4;
pub(crate) const GDKREG: u8 = 5;
pub(crate) const GDKCONST: u8 = 6;

/// Description of an active variable (`Vardesc`).
#[derive(Clone, Debug)]
pub(crate) struct VarDesc<S> {
    pub kind: u8,
    pub ridx: i32,
    pub pidx: i32,
    /// `None` is the collective `global *` declaration
    pub name: Option<S>,
    /// compile-time constant value (kind == RDKCTC)
    pub k: Const<S>,
}

impl<S> VarDesc<S> {
    pub(crate) fn in_reg(&self) -> bool {
        self.kind <= RDKTOCLOSE
    }
    pub(crate) fn is_global(&self) -> bool {
        self.kind >= GDKREG
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LabelDesc<S> {
    pub name: S,
    pub pc: i32,
    pub line: i32,
    pub nactvar: i32,
    pub close: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BlockCnt {
    pub firstlabel: usize,
    pub firstgoto: usize,
    pub nactvar: i32,
    pub upval: bool,
    /// 0 = not a loop; 1 = loop; 2 = loop with pending breaks
    pub isloop: u8,
    pub insidetbc: bool,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum KKey {
    Nil,
    False,
    True,
    Int(i64),
    Flt(u64),
    /// zero floats share one entry (the C code keys them by the `FuncState` pointer)
    Zero,
    Str(Vec<u8>),
}

pub(crate) struct FuncState<S: Copy> {
    pub f: ProtoData<S>,
    pub bl: Vec<BlockCnt>,
    pub kcache: crate::khash::KMap<KKey, usize>,
    pub lasttarget: i32,
    pub previousline: i32,
    /// number of child prototypes opened so far (`np`)
    pub np: usize,
    pub firstlocal: usize,
    pub firstlabel: usize,
    pub nactvar: i32,
    pub freereg: i32,
    pub iwthabs: u8,
    pub needclose: bool,
}

impl<S: Copy> FuncState<S> {
    pub(crate) fn pc(&self) -> i32 {
        self.f.code.len() as i32
    }
}

pub(crate) struct Parser<'a, H: CompilerHost> {
    pub ls: LexState<'a, H>,
    pub fstack: Vec<FuncState<H::Str>>,
    /// storage for variable descriptors; entries at `nact..` are stale but still readable, exactly
    /// like the C `Dyndata.actvar.arr` beyond `n` (the goto code relies on this)
    pub actvar: Vec<VarDesc<H::Str>>,
    pub nact: usize,
    pub gt: Vec<LabelDesc<H::Str>>,
    pub label: Vec<LabelDesc<H::Str>>,
    pub nccalls: u32,
    /// first `luaM_growvector` limit violation seen by `addk` (which cannot fail); reported by the
    /// next `k_code`
    pub limit_error: Option<Vec<u8>>,
}

/// Token helper: is `t` the single-character token `c`?
pub(crate) fn is_char<S>(t: &Token<S>, c: u8) -> bool {
    matches!(t, Token::Char(x) if *x == c)
}
