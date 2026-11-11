//! Pure parts of `ldebug.c` / `lfunc.c` debug support: line lookup, local names, symbolic execution
//! (`getobjname`, `funcnamefromcode`), variable-info formatting and `lua_getinfo` source fields.
//!
//! Everything works over the small [`ProtoView`] trait so it can be verified without a VM
//! ([`ProtoDataView`] for compiler output; the GC `Proto` implements the trait in the runtime).
//! Port target: the vendored `ldebug.c` of the active Lua version (5.5.1 / 5.4.9); the few version
//! differences (`rkname` for `OP_SELF` in 5.4, `OP_GETVARG` in 5.5) are selected with cfg.
#![forbid(unsafe_code)]

use crate::host::CompilerHost;
use crate::instruction::Instruction;
use crate::protodata::{AbsLineInfo, Const, ProtoData, PF_VAHID, PF_VATAB};
use crate::ver::active::{OpCode, OP_PROPS};

/// Event names in `TMS` order (`ltm.h`); `funcnamefromcode` reports them without the `__` prefix.
const TM_NAMES: [&str; 25] = [
    "__index",
    "__newindex",
    "__gc",
    "__mode",
    "__len",
    "__eq",
    "__add",
    "__sub",
    "__mul",
    "__mod",
    "__pow",
    "__div",
    "__idiv",
    "__band",
    "__bor",
    "__bxor",
    "__shl",
    "__shr",
    "__unm",
    "__bnot",
    "__lt",
    "__le",
    "__concat",
    "__call",
    "__close",
];

/// `LUA_IDSIZE`.
pub const LUA_IDSIZE: usize = 60;
/// `MAXIWTHABS`: maximum number of instructions between absolute line-info entries.
pub const MAXIWTHABS: i32 = 128;
/// `ABSLINEINFO` marker in `lineinfo`.
pub const ABSLINEINFO: i8 = -0x80;

/// Read-only view of a function prototype (what `ldebug.c` needs).
pub trait ProtoView {
    fn code(&self) -> &[u32];
    /// Bytes of constant `idx` if it is a string (`ttisstring`), else `None` (also if out of range).
    fn k_str(&self, idx: usize) -> Option<&[u8]>;
    /// Name of upvalue `idx` (`None` = stripped, reported as `?`).
    fn upvalue_name(&self, idx: usize) -> Option<&[u8]>;
    fn num_locvars(&self) -> usize;
    /// `(varname, startpc, endpc)` of local variable `i`.
    fn locvar(&self, i: usize) -> (&[u8], i32, i32);
    fn line_info(&self) -> &[i8];
    fn abs_line_info(&self) -> &[AbsLineInfo];
    fn line_defined(&self) -> i32;
    fn last_line_defined(&self) -> i32;
    fn source(&self) -> Option<&[u8]>;
    fn num_params(&self) -> u8;
    /// Raw `Proto.flag` bits (`PF_VAHID | PF_VATAB`).
    fn flags(&self) -> u8;

    fn is_vararg(&self) -> bool {
        self.flags() & (PF_VAHID | PF_VATAB) != 0
    }
}

/// [`ProtoView`] over compiler output; string handles are resolved through the host.
pub struct ProtoDataView<'a, H: CompilerHost> {
    pub proto: &'a ProtoData<H::Str>,
    pub host: &'a H,
}

impl<'a, H: CompilerHost> ProtoDataView<'a, H> {
    pub fn new(proto: &'a ProtoData<H::Str>, host: &'a H) -> Self {
        ProtoDataView { proto, host }
    }
}

impl<H: CompilerHost> ProtoView for ProtoDataView<'_, H> {
    fn code(&self) -> &[u32] {
        &self.proto.code
    }
    fn k_str(&self, idx: usize) -> Option<&[u8]> {
        match self.proto.k.get(idx) {
            Some(Const::Str(s)) => Some(self.host.bytes(*s)),
            _ => None,
        }
    }
    fn upvalue_name(&self, idx: usize) -> Option<&[u8]> {
        self.proto
            .upvalues
            .get(idx)
            .and_then(|u| u.name)
            .map(|s| self.host.bytes(s))
    }
    fn num_locvars(&self) -> usize {
        self.proto.loc_vars.len()
    }
    fn locvar(&self, i: usize) -> (&[u8], i32, i32) {
        let v = &self.proto.loc_vars[i];
        (self.host.bytes(v.name), v.start_pc as i32, v.end_pc as i32)
    }
    fn line_info(&self) -> &[i8] {
        &self.proto.line_info
    }
    fn abs_line_info(&self) -> &[AbsLineInfo] {
        &self.proto.abs_line_info
    }
    fn line_defined(&self) -> i32 {
        self.proto.line_defined
    }
    fn last_line_defined(&self) -> i32 {
        self.proto.last_line_defined
    }
    fn source(&self) -> Option<&[u8]> {
        self.proto.source.map(|s| self.host.bytes(s))
    }
    fn num_params(&self) -> u8 {
        self.proto.num_params
    }
    fn flags(&self) -> u8 {
        self.proto.flags
    }
}

// ---------------------------------------------------------------------------------------------
// Line information
// ---------------------------------------------------------------------------------------------

/// `getbaseline`: returns `(basepc, baseline)`.
fn getbaseline<P: ProtoView + ?Sized>(f: &P, pc: i32) -> (i32, i32) {
    let abs = f.abs_line_info();
    if abs.is_empty() || pc < abs[0].pc as i32 {
        (-1, f.line_defined())
    } else {
        // (clamped: the estimate is a lower bound only for well-formed debug info)
        let mut i = ((pc / MAXIWTHABS - 1).max(0) as usize).min(abs.len() - 1);
        while i + 1 < abs.len() && pc >= abs[i + 1].pc as i32 {
            i += 1;
        }
        (abs[i].pc as i32, abs[i].line)
    }
}

/// `luaG_getfuncline`: line of instruction `pc`, or `-1` without debug information.
pub fn get_func_line<P: ProtoView + ?Sized>(f: &P, pc: usize) -> i32 {
    let li = f.line_info();
    if li.is_empty() {
        return -1;
    }
    let pc = pc as i32;
    let (mut basepc, mut baseline) = getbaseline(f, pc);
    while basepc < pc {
        basepc += 1;
        let d = li.get(basepc as usize).copied().unwrap_or(0);
        if d != ABSLINEINFO {
            baseline += i32::from(d);
        }
    }
    baseline
}

/// `luaF_getlocalname`: name of the `local_number`-th (1-based) active local at `pc`.
pub fn get_local_name<P: ProtoView + ?Sized>(
    f: &P,
    mut local_number: i32,
    pc: i32,
) -> Option<&[u8]> {
    let mut i = 0;
    while i < f.num_locvars() {
        let (name, start, end) = f.locvar(i);
        if start > pc {
            break;
        }
        if pc < end {
            local_number -= 1;
            if local_number == 0 {
                return Some(name);
            }
        }
        i += 1;
    }
    None
}

/// `nextline` (used by `collectvalidlines`).
fn nextline<P: ProtoView + ?Sized>(p: &P, currentline: i32, pc: usize) -> i32 {
    match p.line_info().get(pc) {
        Some(&d) if d != ABSLINEINFO => currentline + i32::from(d),
        _ => get_func_line(p, pc),
    }
}

/// Active lines of a Lua function (`collectvalidlines`): `None` when there is no debug information.
pub fn collect_valid_lines<P: ProtoView + ?Sized>(p: &P) -> Option<Vec<i32>> {
    let li = p.line_info();
    if li.is_empty() {
        return None;
    }
    let mut currentline = p.line_defined();
    let mut lines = Vec::new();
    let mut i = 0;
    if p.is_vararg() {
        currentline = nextline(p, currentline, 0);
        i = 1;
    }
    while i < li.len() {
        currentline = nextline(p, currentline, i);
        lines.push(currentline);
        i += 1;
    }
    Some(lines)
}

/// `changedline`: did the line change between instructions `oldpc` and `newpc` (`oldpc < newpc`)?
pub fn changed_line<P: ProtoView + ?Sized>(p: &P, oldpc: usize, newpc: usize) -> bool {
    let li = p.line_info();
    if li.is_empty() {
        return false;
    }
    if (newpc as i64 - oldpc as i64) < (MAXIWTHABS / 2) as i64 {
        let mut delta = 0i32;
        let mut pc = oldpc;
        loop {
            pc += 1;
            let Some(&d) = li.get(pc) else { break };
            if d == ABSLINEINFO {
                break;
            }
            delta += i32::from(d);
            if pc == newpc {
                return delta != 0;
            }
        }
    }
    get_func_line(p, oldpc) != get_func_line(p, newpc)
}

// ---------------------------------------------------------------------------------------------
// Symbolic execution
// ---------------------------------------------------------------------------------------------

/// A name found for a register/upvalue/constant: `kind` is one of `local`, `upvalue`, `constant`,
/// `global`, `field`, `method`, `for iterator`, `metamethod`, `hook`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjName<'a> {
    pub kind: &'static str,
    pub name: &'a [u8],
}

const STRLOCAL: &str = "local";
const STRUPVAL: &str = "upvalue";

fn instr<P: ProtoView + ?Sized>(p: &P, pc: usize) -> Instruction {
    Instruction(p.code().get(pc).copied().unwrap_or(0))
}

fn upvalname<P: ProtoView + ?Sized>(p: &P, uv: usize) -> &[u8] {
    p.upvalue_name(uv).unwrap_or(b"?")
}

/// `filterpc`.
fn filterpc(pc: i32, jmptarget: i32) -> i32 {
    if pc < jmptarget {
        -1
    } else {
        pc
    }
}

/// `findsetreg`: last instruction before `lastpc` that modified register `reg` (or -1).
pub fn findsetreg<P: ProtoView + ?Sized>(p: &P, mut lastpc: i32, reg: i32) -> i32 {
    let mut setreg = -1;
    let mut jmptarget = 0;
    if let Some(op) = instr(p, lastpc.max(0) as usize).opcode() {
        if OP_PROPS[op as usize].mm {
            lastpc -= 1;
        }
    }
    let mut pc = 0;
    while pc < lastpc {
        let i = instr(p, pc as usize);
        let a = i.a() as i32;
        let op = i.opcode();
        let change = match op {
            Some(OpCode::Loadnil) => {
                let b = i.b() as i32;
                a <= reg && reg <= a + b
            }
            Some(OpCode::Tforcall) => reg >= a + 2,
            Some(OpCode::Call) | Some(OpCode::Tailcall) => reg >= a,
            Some(OpCode::Jmp) => {
                let b = i.sj();
                let dest = pc + 1 + b;
                if dest <= lastpc && dest > jmptarget {
                    jmptarget = dest;
                }
                false
            }
            Some(op) => OP_PROPS[op as usize].a && reg == a,
            None => false,
        };
        if change {
            setreg = filterpc(pc, jmptarget);
        }
        pc += 1;
    }
    setreg
}

/// `kname`: `("constant", string)` for a string constant, else `(None, "?")`.
fn kname<P: ProtoView + ?Sized>(p: &P, index: usize) -> (Option<&'static str>, &[u8]) {
    match p.k_str(index) {
        Some(s) => (Some("constant"), s),
        None => (None, b"?"),
    }
}

/// `basicgetobjname`: updates `ppc` like the C pointer argument.
fn basicgetobjname<'a, P: ProtoView + ?Sized>(
    p: &'a P,
    ppc: &mut i32,
    reg: i32,
) -> Option<ObjName<'a>> {
    let mut pc = *ppc;
    if let Some(name) = get_local_name(p, reg + 1, pc) {
        return Some(ObjName {
            kind: STRLOCAL,
            name,
        });
    }
    pc = findsetreg(p, pc, reg);
    *ppc = pc;
    if pc != -1 {
        let i = instr(p, pc as usize);
        match i.opcode() {
            Some(OpCode::Move) => {
                let b = i.b() as i32;
                if b < i.a() as i32 {
                    return basicgetobjname(p, ppc, b);
                }
            }
            Some(OpCode::Getupval) => {
                return Some(ObjName {
                    kind: STRUPVAL,
                    name: upvalname(p, i.b() as usize),
                });
            }
            Some(OpCode::Loadk) => return kname_obj(p, i.bx() as usize),
            Some(OpCode::Loadkx) => {
                return kname_obj(p, instr(p, pc as usize + 1).ax() as usize);
            }
            _ => {}
        }
    }
    None
}

fn kname_obj<P: ProtoView + ?Sized>(p: &P, index: usize) -> Option<ObjName<'_>> {
    match kname(p, index) {
        (Some(kind), name) => Some(ObjName { kind, name }),
        (None, _) => None,
    }
}

/// `rname`: name for register `c` (only if it holds a string constant), else `?`.
fn rname<P: ProtoView + ?Sized>(p: &P, mut pc: i32, c: i32) -> &[u8] {
    match basicgetobjname(p, &mut pc, c) {
        Some(o) if o.kind.starts_with('c') => o.name,
        _ => b"?",
    }
}

/// `rkname` (5.4 only): name for a `C` operand that may be a constant or a register.
#[cfg(rustynail_lua54)]
fn rkname<P: ProtoView + ?Sized>(p: &P, pc: i32, i: Instruction) -> &[u8] {
    let c = i.c() as usize;
    if i.k() != 0 {
        kname(p, c).1
    } else {
        rname(p, pc, c as i32)
    }
}

/// `isEnv`: `"global"` if the indexed table is the environment variable, else `"field"`.
fn is_env<P: ProtoView + ?Sized>(p: &P, mut pc: i32, i: Instruction, isup: bool) -> &'static str {
    let t = i.b() as i32;
    let name: Option<&[u8]> = if isup {
        Some(upvalname(p, t as usize))
    } else {
        match basicgetobjname(p, &mut pc, t) {
            Some(o) if o.kind == STRLOCAL || o.kind == STRUPVAL => Some(o.name),
            _ => None,
        }
    };
    if name == Some(b"_ENV") {
        "global"
    } else {
        "field"
    }
}

/// `getobjname`: name and kind of the value in register `reg` at instruction `lastpc`.
pub fn getobjname<P: ProtoView + ?Sized>(p: &P, lastpc: i32, reg: i32) -> Option<ObjName<'_>> {
    let mut lastpc = lastpc;
    if let Some(o) = basicgetobjname(p, &mut lastpc, reg) {
        return Some(o);
    }
    if lastpc == -1 {
        return None;
    }
    let i = instr(p, lastpc as usize);
    match i.opcode()? {
        OpCode::Gettabup => {
            let (_, name) = kname(p, i.c() as usize);
            Some(ObjName {
                kind: is_env(p, lastpc, i, true),
                name,
            })
        }
        OpCode::Gettable => {
            let name = rname(p, lastpc, i.c() as i32);
            Some(ObjName {
                kind: is_env(p, lastpc, i, false),
                name,
            })
        }
        #[cfg(not(rustynail_lua54))]
        OpCode::Getvarg => {
            let name = rname(p, lastpc, i.c() as i32);
            Some(ObjName {
                kind: is_env(p, lastpc, i, false),
                name,
            })
        }
        OpCode::Geti => Some(ObjName {
            kind: "field",
            name: b"integer index",
        }),
        OpCode::Getfield => {
            let (_, name) = kname(p, i.c() as usize);
            Some(ObjName {
                kind: is_env(p, lastpc, i, false),
                name,
            })
        }
        OpCode::SelfOp => {
            #[cfg(rustynail_lua54)]
            let name = rkname(p, lastpc, i);
            #[cfg(not(rustynail_lua54))]
            let name = kname(p, i.c() as usize).1;
            Some(ObjName {
                kind: "method",
                name,
            })
        }
        _ => None,
    }
}

/// `funcnamefromcode`: name of the function being called by instruction `pc` (a call or an
/// instruction that may invoke a metamethod).
pub fn funcnamefromcode<P: ProtoView + ?Sized>(p: &P, pc: usize) -> Option<ObjName<'_>> {
    let i = instr(p, pc);
    let tm: usize = match i.opcode()? {
        OpCode::Call | OpCode::Tailcall => return getobjname(p, pc as i32, i.a() as i32),
        OpCode::Tforcall => {
            return Some(ObjName {
                kind: "for iterator",
                name: b"for iterator",
            })
        }
        OpCode::SelfOp | OpCode::Gettabup | OpCode::Gettable | OpCode::Geti | OpCode::Getfield => 0,
        OpCode::Settabup | OpCode::Settable | OpCode::Seti | OpCode::Setfield => 1,
        OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink => i.c() as usize,
        OpCode::Unm => 18,
        OpCode::Bnot => 19,
        OpCode::Len => 4,
        OpCode::Concat => 22,
        OpCode::Eq => 5,
        OpCode::Lt | OpCode::Lti | OpCode::Gti => 20,
        OpCode::Le | OpCode::Lei | OpCode::Gei => 21,
        OpCode::Close | OpCode::Return => 24,
        _ => return None,
    };
    let full = TM_NAMES.get(tm)?;
    Some(ObjName {
        kind: "metamethod",
        name: &full.as_bytes()[2..],
    })
}

/// `funcnamefromcall` for the non-Lua-caller cases that need no frame data.
pub fn funcname_hook() -> ObjName<'static> {
    ObjName {
        kind: "hook",
        name: b"?",
    }
}

/// Name reported for a function called as a finalizer.
pub fn funcname_finalizer() -> ObjName<'static> {
    ObjName {
        kind: "metamethod",
        name: b"__gc",
    }
}

// ---------------------------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------------------------

/// `formatvarinfo`: `" (kind 'name')"`, or empty when there is no information.
pub fn format_varinfo(o: Option<ObjName<'_>>) -> Vec<u8> {
    match o {
        None => Vec::new(),
        Some(o) => {
            let mut v = Vec::with_capacity(o.name.len() + o.kind.len() + 5);
            v.extend_from_slice(b" (");
            v.extend_from_slice(o.kind.as_bytes());
            v.extend_from_slice(b" '");
            v.extend_from_slice(o.name);
            v.extend_from_slice(b"')");
            v
        }
    }
}

/// `varinfo` for a value found in register `reg` of the function running at `pc`.
pub fn varinfo_register<P: ProtoView + ?Sized>(p: &P, pc: usize, reg: usize) -> Vec<u8> {
    format_varinfo(getobjname(p, pc as i32, reg as i32))
}

/// `varinfo` for a value that is upvalue `idx` of the running closure (`getupvalname`).
pub fn varinfo_upvalue<P: ProtoView + ?Sized>(p: &P, idx: usize) -> Vec<u8> {
    format_varinfo(Some(ObjName {
        kind: STRUPVAL,
        name: upvalname(p, idx),
    }))
}

/// `luaG_callerror` extra text when the caller's call instruction gives a name (`funcnamefromcode`),
/// falling back to `fallback` (the `varinfo` text) otherwise.
pub fn callerror_extra<P: ProtoView + ?Sized>(p: &P, pc: usize, fallback: Vec<u8>) -> Vec<u8> {
    match funcnamefromcode(p, pc) {
        Some(o) => format_varinfo(Some(o)),
        None => fallback,
    }
}

/// `luaG_errnnil` message: `global 'name' already defined` (`k` is the instruction's `Bx`, 0 = unknown).
pub fn errnnil_message<P: ProtoView + ?Sized>(p: &P, k: usize) -> Vec<u8> {
    let name: &[u8] = if k > 0 { kname(p, k - 1).1 } else { b"?" };
    let mut m = b"global '".to_vec();
    m.extend_from_slice(name);
    m.extend_from_slice(b"' already defined");
    m
}

// ---------------------------------------------------------------------------------------------
// lua_getinfo source fields
// ---------------------------------------------------------------------------------------------

/// The `S` fields of `lua_Debug`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceInfo {
    pub source: Vec<u8>,
    pub short_src: Vec<u8>,
    pub line_defined: i32,
    pub last_line_defined: i32,
    pub what: &'static str,
}

/// `funcinfo` for a Lua function.
pub fn lua_funcinfo<P: ProtoView + ?Sized>(p: &P) -> SourceInfo {
    let source = p.source().map_or_else(|| b"=?".to_vec(), |s| s.to_vec());
    let short_src = chunk_id(&source);
    SourceInfo {
        short_src,
        source,
        line_defined: p.line_defined(),
        last_line_defined: p.last_line_defined(),
        what: if p.line_defined() == 0 { "main" } else { "Lua" },
    }
}

/// `funcinfo` for a C function.
pub fn c_funcinfo() -> SourceInfo {
    SourceInfo {
        source: b"=[C]".to_vec(),
        short_src: b"[C]".to_vec(),
        line_defined: -1,
        last_line_defined: -1,
        what: "C",
    }
}

/// `luaG_addinfo`: prefix `msg` with `short_src:line:` (or `?:?:` without source information).
pub fn add_info(msg: &[u8], src: Option<&[u8]>, line: i32) -> Vec<u8> {
    let mut v = Vec::new();
    match src {
        #[cfg(not(rustynail_lua54))]
        None => v.extend_from_slice(b"?:?: "),
        // 5.4: "?" for the source, but the line is still printed (-1 without line info)
        #[cfg(rustynail_lua54)]
        None => {
            v.extend_from_slice(b"?:");
            v.extend_from_slice(line.to_string().as_bytes());
            v.extend_from_slice(b": ");
        }
        Some(s) => {
            v.extend_from_slice(&chunk_id(s));
            v.push(b':');
            v.extend_from_slice(line.to_string().as_bytes());
            v.extend_from_slice(b": ");
        }
    }
    v.extend_from_slice(msg);
    v
}

/// `luaO_chunkid`: printable chunk name from a `source` string (`@file`, `=name`, or literal source).
pub fn chunk_id(source: &[u8]) -> Vec<u8> {
    let mut buf = [0u8; LUA_IDSIZE];
    let n = chunk_id_buf(source, &mut buf);
    buf[..n].to_vec()
}

/// Appends `s` to `out` at `*n` (the callers below never exceed `LUA_IDSIZE - 1` bytes in total).
#[inline]
fn put(out: &mut [u8; LUA_IDSIZE], n: &mut usize, s: &[u8]) {
    out[*n..*n + s.len()].copy_from_slice(s);
    *n += s.len();
}

/// [`chunk_id`] without allocation: writes into `out` (the result is at most `LUA_IDSIZE - 1` bytes) and returns its length.
pub fn chunk_id_buf(source: &[u8], out: &mut [u8; LUA_IDSIZE]) -> usize {
    let srclen = source.len();
    let mut n = 0usize;
    match source.first() {
        Some(b'=') => {
            let rest = &source[1..];
            if srclen <= LUA_IDSIZE {
                put(out, &mut n, rest);
            } else {
                put(out, &mut n, &rest[..LUA_IDSIZE - 1]);
            }
        }
        Some(b'@') => {
            let rest = &source[1..];
            if srclen <= LUA_IDSIZE {
                put(out, &mut n, rest);
            } else {
                put(out, &mut n, b"...");
                let keep = LUA_IDSIZE - 3 - 1;
                put(out, &mut n, &source[srclen - keep..]);
            }
        }
        _ => {
            const PRE: &[u8] = b"[string \"";
            const RETS: &[u8] = b"...";
            const POS: &[u8] = b"\"]";
            let nl = source.iter().position(|&c| c == b'\n');
            put(out, &mut n, PRE);
            let bufflen = LUA_IDSIZE - (PRE.len() + RETS.len() + POS.len()) - 1;
            if srclen < bufflen && nl.is_none() {
                put(out, &mut n, source);
            } else {
                let mut m = nl.unwrap_or(srclen);
                if m > bufflen {
                    m = bufflen;
                }
                put(out, &mut n, &source[..m]);
                put(out, &mut n, RETS);
            }
            put(out, &mut n, POS);
        }
    }
    match out[..n].iter().position(|&c| c == 0) {
        Some(i) => i,
        None => n,
    }
}
