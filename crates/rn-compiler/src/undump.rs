//! Binary chunk loader (port of `lundump.c`, Lua 5.5 format).
//!
//! Robustness: every count read from the (untrusted) input is validated against the number of bytes
//! that remain before anything is allocated, nesting depth is bounded, and no input can cause a
//! panic or an unbounded loop. Error messages are identical to the reference
//! (`<name>: bad binary format (<why>)`).
use crate::CompileError;
use rn_core::host::CompilerHost;
use rn_core::protodata::{AbsLineInfo, Const, LocVar, ProtoData, UpvalDesc};

/// First byte of every binary chunk (`LUA_SIGNATURE[0]`).
pub const LUA_SIGNATURE: &[u8] = b"\x1bLua";
/// `LUAC_DATA`
pub const LUAC_DATA: &[u8] = b"\x19\x93\r\n\x1a\n";
#[cfg(not(rustynail_lua54))]
pub const LUAC_INT: i64 = -0x5678;
/// 5.4: positive (`lundump.h`).
#[cfg(rustynail_lua54)]
pub const LUAC_INT: i64 = 0x5678;
#[cfg_attr(rustynail_lua54, allow(dead_code))]
pub const LUAC_INST: u32 = 0x1234_5678;
#[cfg(not(rustynail_lua54))]
pub const LUAC_NUM: f64 = -370.5;
#[cfg(rustynail_lua54)]
pub const LUAC_NUM: f64 = 370.5;
pub const LUAC_FORMAT: u8 = 0;
/// `LUAC_VERSION`: major*16+minor of the active Lua version.
#[cfg(not(rustynail_lua54))]
pub const LUAC_VERSION: u8 = 0x55;
#[cfg(rustynail_lua54)]
pub const LUAC_VERSION: u8 = 0x54;

pub(crate) const LUA_VNIL: u8 = 0;
pub(crate) const LUA_VFALSE: u8 = 1;
pub(crate) const LUA_VTRUE: u8 = 17;
pub(crate) const LUA_VNUMINT: u8 = 3;
pub(crate) const LUA_VNUMFLT: u8 = 19;
pub(crate) const LUA_VSHRSTR: u8 = 4;
pub(crate) const LUA_VLNGSTR: u8 = 20;

/// Maximum prototype nesting accepted (the reference relies on the C stack; we bound it explicitly).
const MAX_DEPTH: u32 = 200;

mod imp {
    use super::*;

    struct Loader<'a, H: CompilerHost> {
        host: &'a mut H,
        b: &'a [u8],
        pos: usize,
        name: Vec<u8>,
        strings: Vec<H::Str>,
    }

    type R<T> = Result<T, CompileError>;

    impl<H: CompilerHost> Loader<'_, H> {
        fn error<T>(&self, why: &str) -> R<T> {
            let mut m = cstr(&self.name).to_vec();
            m.extend_from_slice(b": bad binary format (");
            m.extend_from_slice(why.as_bytes());
            m.push(b')');
            Err(CompileError { message: m })
        }

        fn remaining(&self) -> usize {
            self.b.len().saturating_sub(self.pos)
        }

        fn block(&mut self, n: usize) -> R<&[u8]> {
            if n > self.remaining() {
                return self.error("truncated chunk");
            }
            let s = &self.b[self.pos..self.pos + n];
            self.pos += n;
            Ok(s)
        }

        #[cfg(not(rustynail_lua54))]
        fn align(&mut self, align: usize) -> R<()> {
            let padding = align - self.pos % align;
            if padding < align {
                self.block(padding)?;
            }
            Ok(())
        }

        fn byte(&mut self) -> R<u8> {
            match self.b.get(self.pos) {
                Some(&c) => {
                    self.pos += 1;
                    Ok(c)
                }
                None => self.error("truncated chunk"),
            }
        }

        /// 5.5: groups of 7 bits, most significant first, high bit set on every byte but the last.
        #[cfg(not(rustynail_lua54))]
        fn varint(&mut self, limit: u64) -> R<u64> {
            let mut x: u64 = 0;
            let limit = limit >> 7;
            loop {
                let b = self.byte()?;
                if x > limit {
                    return self.error("integer overflow");
                }
                x = (x << 7) | u64::from(b & 0x7f);
                if b & 0x80 == 0 {
                    return Ok(x);
                }
            }
        }

        /// 5.4 `loadUnsigned`: groups of 7 bits, most significant first, high bit set on the LAST byte.
        #[cfg(rustynail_lua54)]
        fn varint(&mut self, limit: u64) -> R<u64> {
            let mut x: u64 = 0;
            let limit = limit >> 7;
            loop {
                let b = self.byte()?;
                if x >= limit {
                    return self.error("integer overflow");
                }
                x = (x << 7) | u64::from(b & 0x7f);
                if b & 0x80 != 0 {
                    return Ok(x);
                }
            }
        }

        fn size(&mut self) -> R<u64> {
            self.varint(if cfg!(rustynail_lua54) {
                u64::MAX
            } else {
                i64::MAX as u64
            })
        }

        fn int(&mut self) -> R<i32> {
            Ok(self.varint(i32::MAX as u64)? as i32)
        }

        /// A count of elements of `elem` bytes each, validated against the remaining input.
        fn count(&mut self, elem: usize) -> R<usize> {
            let n = self.int()? as usize;
            match n.checked_mul(elem) {
                Some(total) if total <= self.remaining() => Ok(n),
                _ => self.error("truncated chunk"),
            }
        }

        fn number(&mut self) -> R<f64> {
            let s = self.block(8)?;
            let mut a = [0u8; 8];
            a.copy_from_slice(s);
            Ok(f64::from_le_bytes(a))
        }

        #[cfg(not(rustynail_lua54))]
        fn integer(&mut self) -> R<i64> {
            let cx = self.varint(u64::MAX)?;
            Ok(if cx & 1 != 0 {
                !(cx >> 1) as i64
            } else {
                (cx >> 1) as i64
            })
        }

        #[cfg(rustynail_lua54)]
        fn integer(&mut self) -> R<i64> {
            let s = self.block(8)?;
            let mut a = [0u8; 8];
            a.copy_from_slice(s);
            Ok(i64::from_le_bytes(a))
        }

        fn intern(&mut self, bytes_range: (usize, usize)) -> R<H::Str> {
            let (s, e) = bytes_range;
            match self.host.intern(&self.b[s..e]) {
                Ok(h) => Ok(h),
                Err(_) => Err(CompileError {
                    message: b"not enough memory".to_vec(),
                }),
            }
        }

        /// `loadString`: nullable string. 5.5 also has back-references to previously loaded strings.
        fn string(&mut self) -> R<Option<H::Str>> {
            let size = self.size()?;
            if size == 0 {
                #[cfg(rustynail_lua54)]
                return Ok(None);
                #[cfg(not(rustynail_lua54))]
                {
                    let idx = self.varint(u64::MAX)?;
                    if idx == 0 {
                        return Ok(None);
                    }
                    return match usize::try_from(idx - 1)
                        .ok()
                        .and_then(|i| self.strings.get(i))
                    {
                        Some(&s) => Ok(Some(s)),
                        None => self.error("invalid string index"),
                    };
                }
            }
            let size = size - 1;
            // the 5.5 dump includes the terminating '\0'; 5.4 does not
            let nul = usize::from(!cfg!(rustynail_lua54));
            let total = match usize::try_from(size).ok().and_then(|n| n.checked_add(nul)) {
                Some(t) if t <= self.remaining() => t,
                _ => return self.error("truncated chunk"),
            };
            let start = self.pos;
            self.pos += total;
            let ts = self.intern((start, start + total - nul))?;
            self.strings.push(ts);
            Ok(Some(ts))
        }

        fn code(&mut self) -> R<Vec<u32>> {
            let n = self.count(4)?;
            #[cfg(not(rustynail_lua54))]
            self.align(4)?;
            let raw = self.block(n * 4)?;
            let code: Vec<u32> = raw
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect();
            // the interpreter relies on every opcode being a defined one
            let nop = rn_core::ver::active::NUM_OPCODES as u32;
            if code.iter().any(|i| (i & 0x7f) >= nop) {
                return self.error("invalid opcode");
            }
            Ok(code)
        }

        fn constants(&mut self) -> R<Vec<Const<H::Str>>> {
            let n = self.count(1)?;
            let mut k = Vec::with_capacity(n.min(1024));
            for _ in 0..n {
                let t = self.byte()?;
                k.push(match t {
                    LUA_VNIL => Const::Nil,
                    LUA_VFALSE => Const::False,
                    LUA_VTRUE => Const::True,
                    LUA_VNUMFLT => Const::Float(self.number()?),
                    LUA_VNUMINT => Const::Int(self.integer()?),
                    LUA_VSHRSTR | LUA_VLNGSTR => match self.string()? {
                        Some(s) => Const::Str(s),
                        None => return self.error("bad format for constant string"),
                    },
                    _ => return self.error("invalid constant"),
                });
            }
            Ok(k)
        }

        fn upvalues(&mut self) -> R<Vec<UpvalDesc<H::Str>>> {
            let n = self.count(3)?;
            let mut v = Vec::with_capacity(n.min(1024));
            for _ in 0..n {
                let in_stack = self.byte()? != 0;
                let idx = self.byte()?;
                let kind = self.byte()?;
                v.push(UpvalDesc {
                    name: None,
                    in_stack,
                    idx,
                    kind,
                });
            }
            Ok(v)
        }

        fn protos(&mut self, depth: u32, psource: Option<H::Str>) -> R<Vec<ProtoData<H::Str>>> {
            // every function takes well over one byte
            let n = self.count(1)?;
            let mut v = Vec::with_capacity(n.min(64));
            for _ in 0..n {
                v.push(self.function(depth + 1, psource)?);
            }
            Ok(v)
        }

        fn debug(&mut self, nup: usize, p: &mut ProtoData<H::Str>) -> R<()> {
            let n = self.count(1)?;
            p.line_info = self.block(n)?.iter().map(|&b| b as i8).collect();
            #[cfg(not(rustynail_lua54))]
            {
                let n = self.int()? as usize;
                if n > 0 {
                    if n.checked_mul(8).is_none_or(|t| t > self.remaining()) {
                        return self.error("truncated chunk");
                    }
                    self.align(4)?;
                    let raw = self.block(n * 8)?;
                    p.abs_line_info = raw
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|c| AbsLineInfo {
                            pc: u32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                            line: i32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                        })
                        .collect();
                }
            }
            #[cfg(rustynail_lua54)]
            {
                // two varints (pc, line) per entry
                let n = self.count(2)?;
                p.abs_line_info = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    let pc = self.int()? as u32;
                    let line = self.int()?;
                    p.abs_line_info.push(AbsLineInfo { pc, line });
                }
            }
            // each local variable takes at least 3 bytes (string ref + two ints)
            let n = self.count(3)?;
            p.loc_vars = Vec::with_capacity(n.min(1024));
            for _ in 0..n {
                let name = match self.string()? {
                    Some(s) => s,
                    None => return self.error("bad format for local variable name"),
                };
                let start_pc = self.int()? as u32;
                let end_pc = self.int()? as u32;
                p.loc_vars.push(LocVar {
                    name,
                    start_pc,
                    end_pc,
                });
            }
            let mut n = self.int()? as usize;
            if n != 0 {
                n = nup;
            }
            if n > self.remaining() {
                return self.error("truncated chunk");
            }
            for i in 0..n {
                let name = self.string()?;
                if let Some(u) = p.upvalues.get_mut(i) {
                    u.name = name;
                }
            }
            Ok(())
        }

        /// `psource`: the parent's source (5.4 children without their own source inherit it).
        fn function(
            &mut self,
            depth: u32,
            #[allow(unused_variables)] psource: Option<H::Str>,
        ) -> R<ProtoData<H::Str>> {
            if depth > MAX_DEPTH {
                return self.error("function nesting too deep");
            }
            #[cfg(rustynail_lua54)]
            let source = self.string()?.or(psource);
            let line_defined = self.int()?;
            let last_line_defined = self.int()?;
            let num_params = self.byte()?;
            let flag = self.byte()?;
            let max_stack_size = self.byte()?;
            let code = self.code()?;
            let k = self.constants()?;
            let upvalues = self.upvalues()?;
            #[cfg(rustynail_lua54)]
            let protos = self.protos(depth, source)?;
            #[cfg(not(rustynail_lua54))]
            let protos = self.protos(depth, None)?;
            #[cfg(not(rustynail_lua54))]
            let source = self.string()?;
            let mut p = ProtoData {
                num_params,
                // 5.5: PF_VAHID (1) | PF_VATAB (2); PF_FIXED is runtime-only. 5.4: plain `isvararg` byte.
                flags: if cfg!(rustynail_lua54) {
                    u8::from(flag != 0)
                } else {
                    flag & 3
                },
                max_stack_size,
                line_defined,
                last_line_defined,
                code,
                k,
                protos,
                upvalues,
                line_info: Vec::new(),
                abs_line_info: Vec::new(),
                loc_vars: Vec::new(),
                source,
            };
            let nup = p.upvalues.len();
            self.debug(nup, &mut p)?;
            Ok(p)
        }

        fn check_literal(&mut self, s: &[u8], msg: &str) -> R<()> {
            let got = self.block(s.len())?;
            if got != s {
                return self.error(msg);
            }
            Ok(())
        }

        #[cfg_attr(rustynail_lua54, allow(dead_code))]
        fn num_error<T>(&self, what: &str, tname: &str) -> R<T> {
            self.error(&format!("{tname} {what} mismatch"))
        }

        fn check_size(&mut self, size: u8, tname: &str) -> R<()> {
            if self.byte()? != size {
                return self.num_error("size", tname);
            }
            Ok(())
        }

        #[cfg(not(rustynail_lua54))]
        fn check_header(&mut self) -> R<()> {
            self.check_literal(&LUA_SIGNATURE[1..], "not a binary chunk")?;
            if self.byte()? != LUAC_VERSION {
                return self.error("version mismatch");
            }
            if self.byte()? != LUAC_FORMAT {
                return self.error("format mismatch");
            }
            self.check_literal(LUAC_DATA, "corrupted chunk")?;
            self.check_size(4, "int")?;
            let v = self.block(4)?;
            if v != (LUAC_INT as i32).to_le_bytes() {
                return self.num_error("format", "int");
            }
            self.check_size(4, "instruction")?;
            let v = self.block(4)?;
            if v != LUAC_INST.to_le_bytes() {
                return self.num_error("format", "instruction");
            }
            self.check_size(8, "Lua integer")?;
            let v = self.block(8)?;
            if v != LUAC_INT.to_le_bytes() {
                return self.num_error("format", "Lua integer");
            }
            self.check_size(8, "Lua number")?;
            let v = self.block(8)?;
            if v != LUAC_NUM.to_le_bytes() {
                return self.num_error("format", "Lua number");
            }
            Ok(())
        }

        #[cfg(rustynail_lua54)]
        fn check_header(&mut self) -> R<()> {
            self.check_literal(&LUA_SIGNATURE[1..], "not a binary chunk")?;
            if self.byte()? != LUAC_VERSION {
                return self.error("version mismatch");
            }
            if self.byte()? != LUAC_FORMAT {
                return self.error("format mismatch");
            }
            self.check_literal(LUAC_DATA, "corrupted chunk")?;
            self.check_size(4, "Instruction")?;
            self.check_size(8, "lua_Integer")?;
            self.check_size(8, "lua_Number")?;
            if self.integer()? != LUAC_INT {
                return self.error("integer format mismatch");
            }
            if self.number()? != LUAC_NUM {
                return self.error("float format mismatch");
            }
            Ok(())
        }
    }

    /// Bytes of `s` up to the first NUL (C-string view used by `%s`).
    fn cstr(s: &[u8]) -> &[u8] {
        match s.iter().position(|&c| c == 0) {
            Some(i) => &s[..i],
            None => s,
        }
    }

    pub fn undump<H: CompilerHost>(
        host: &mut H,
        bytes: &[u8],
        chunk_name: &[u8],
    ) -> Result<ProtoData<H::Str>, CompileError> {
        let name: &[u8] = match chunk_name.first() {
            Some(b'@') | Some(b'=') => &chunk_name[1..],
            Some(&c) if c == LUA_SIGNATURE[0] => b"binary string",
            _ => chunk_name,
        };
        let mut l = Loader {
            host,
            b: bytes,
            pos: 1.min(bytes.len()),
            name: name.to_vec(),
            strings: Vec::new(),
        };
        if bytes.is_empty() {
            return l.error("truncated chunk");
        }
        l.check_header()?;
        let nup = l.byte()?;
        let p = l.function(0, None)?;
        if usize::from(nup) != p.upvalues.len() {
            return l.error("corrupted chunk");
        }
        Ok(p)
    }
}

/// Load a precompiled chunk. `bytes` includes the leading ESC byte. `chunk_name` follows the usual
/// `@file` / `=name` / source-string conventions (used only for error messages).
pub fn undump<H: CompilerHost>(
    host: &mut H,
    bytes: &[u8],
    chunk_name: &[u8],
) -> Result<ProtoData<H::Str>, CompileError> {
    imp::undump(host, bytes, chunk_name)
}
