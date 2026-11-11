//! Lua 5.4 parser and code generator (ports of `lparser.c` / `lcode.c`).
//!
//! The C code threads a `FuncState` chain through `prev` pointers; here the active functions live in
//! `Parser::fstack` and parser helpers take a function index where the C code takes an arbitrary `fs`.
//! Code-generation helpers (`k_*`, from `lcode.c`) always act on the innermost function.
use crate::lexer::{LexState, Token};
use crate::CompileError;
use rn_core::host::CompilerHost;
use rn_core::protodata::{Const, ProtoData};
use std::collections::HashMap;

mod codegen;
mod parser;

pub use parser::compile;

/// Parser results. The error is boxed so that `R<()>`/`R<bool>`/`R<i32>` come back in registers
/// (a 24-byte `Result<_, CompileError>` is returned through memory by every parser function).
pub(crate) type R<T> = Result<T, Box<CompileError>>;

/// `Err` of an [`R`] (cold: errors end the compilation).
#[cold]
#[inline(never)]
pub(crate) fn perr<T>(e: CompileError) -> R<T> {
    Err(Box::new(e))
}

pub(crate) const NO_JUMP: i32 = -1;
pub(crate) const MAXVARS: i32 = 200;
pub(crate) const MAXREGS: i32 = 255;
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
    Upval,
    Const,
    Indexed,
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
            Local | Upval | Const | Indexed | IndexUp | IndexI | IndexStr
        )
    }
    pub(crate) fn is_indexed(self) -> bool {
        use ExpKind::*;
        matches!(self, Indexed | IndexUp | IndexI | IndexStr)
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
pub(crate) const RDKTOCLOSE: u8 = 2;
pub(crate) const RDKCTC: u8 = 3;

/// Description of an active local variable (`Vardesc`).
#[derive(Clone, Debug)]
pub(crate) struct VarDesc<S> {
    pub kind: u8,
    pub ridx: i32,
    pub pidx: i32,
    pub name: S,
    /// compile-time constant value (kind == RDKCTC)
    pub k: Const<S>,
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
    pub isloop: bool,
    pub insidetbc: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum KKey {
    /// nil cannot be a table key: the scanner table itself stands for it
    Nil,
    False,
    True,
    /// integers, and floats with an integral value (the table normalizes float keys)
    Int(i64),
    /// non-integral float keys (bit pattern)
    Flt(u64),
    /// string keys by object identity (`CompilerHost::str_id`: the scanner table holds every
    /// string of the chunk once, so identity is content equality there)
    Str(usize),
}

pub(crate) use crate::khash::KHasher;

pub(crate) type KCache = HashMap<KKey, usize, std::hash::BuildHasherDefault<KHasher>>;

pub(crate) struct FuncState<S: Copy> {
    pub f: ProtoData<S>,
    pub bl: Vec<BlockCnt>,
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
    /// the scanner table (`ls->h`) used as constant cache; shared by all functions, so cached
    /// indices must be validated against the current function's constant list
    pub kcache: KCache,
}

/// `t == f` for a fixed token `f` (a symbol, a reserved word or `Char`): the derived `PartialEq`
/// is an out-of-line call comparing payloads, this is a tag compare.
#[inline]
pub(crate) fn tok_is<S>(t: &Token<S>, f: &Token<S>) -> bool {
    debug_assert!(!matches!(
        f,
        Token::Name(_) | Token::Str(_) | Token::Int(_) | Token::Flt(_)
    ));
    match (t, f) {
        (Token::Char(a), Token::Char(b)) => a == b,
        _ => std::mem::discriminant(t) == std::mem::discriminant(f),
    }
}

/// Token helper: is `t` the single-character token `c`?
pub(crate) fn is_char<S>(t: &Token<S>, c: u8) -> bool {
    matches!(t, Token::Char(x) if *x == c)
}
