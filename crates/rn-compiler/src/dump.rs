//! Binary chunk writer (port of `ldump.c`, Lua 5.5 and 5.4 formats, selected by cfg). Output is byte-identical to the reference.
use rn_core::host::CompilerHost;
use rn_core::protodata::{Const, ProtoData};

mod imp {
    use super::*;
    use crate::undump::*;
    struct Dumper<'a, H: CompilerHost> {
        host: &'a H,
        out: Vec<u8>,
        strip: bool,
        #[cfg(not(rustynail_lua54))]
        /// Strings already written, by object identity (address, length), like `D->h` in `ldump.c`
        /// (short strings are interned, so equal short strings are one object).
        seen: crate::khash::KMap<(usize, usize), u64>,
        #[cfg(not(rustynail_lua54))]
        nstr: u64,
    }

    impl<'a, H: CompilerHost> Dumper<'a, H> {
        fn byte(&mut self, b: u8) {
            self.out.push(b);
        }

        #[cfg(not(rustynail_lua54))]
        fn varint(&mut self, mut x: u64) {
            let mut buf = [0u8; 10];
            let mut n = 1;
            buf[9] = (x & 0x7f) as u8;
            loop {
                x >>= 7;
                if x == 0 {
                    break;
                }
                n += 1;
                buf[10 - n] = ((x & 0x7f) as u8) | 0x80;
            }
            self.out.extend_from_slice(&buf[10 - n..]);
        }

        /// 5.4 `dumpSize`: 7-bit groups, most significant first, high bit set on the last byte.
        #[cfg(rustynail_lua54)]
        fn varint(&mut self, mut x: u64) {
            let mut buf = [0u8; 10];
            let mut n = 0;
            loop {
                n += 1;
                buf[10 - n] = (x & 0x7f) as u8;
                x >>= 7;
                if x == 0 {
                    break;
                }
            }
            buf[9] |= 0x80;
            self.out.extend_from_slice(&buf[10 - n..]);
        }

        fn int(&mut self, x: i32) {
            self.varint(u64::from(x as u32));
        }

        #[cfg(not(rustynail_lua54))]
        fn integer(&mut self, x: i64) {
            let ux = x as u64;
            let cx = if x >= 0 { 2 * ux } else { 2 * !ux + 1 };
            self.varint(cx);
        }

        #[cfg(rustynail_lua54)]
        fn integer(&mut self, x: i64) {
            self.out.extend_from_slice(&x.to_le_bytes());
        }

        #[cfg(not(rustynail_lua54))]
        fn align(&mut self, align: usize) {
            let padding = align - self.out.len() % align;
            if padding < align {
                self.out.resize(self.out.len() + padding, 0);
            }
        }

        #[cfg(not(rustynail_lua54))]
        fn string(&mut self, s: Option<H::Str>) {
            match s {
                None => {
                    self.varint(0);
                    self.varint(0);
                }
                Some(s) => {
                    let host: &'a H = self.host;
                    let bytes: &'a [u8] = host.bytes(s);
                    let key = (bytes.as_ptr() as usize, bytes.len());
                    if let Some(&idx) = self.seen.get(&key) {
                        self.varint(0);
                        self.varint(idx);
                    } else {
                        self.varint(bytes.len() as u64 + 1);
                        self.out.extend_from_slice(bytes);
                        self.out.push(0);
                        self.nstr += 1;
                        self.seen.insert(key, self.nstr);
                    }
                }
            }
        }

        #[cfg(rustynail_lua54)]
        fn string(&mut self, s: Option<H::Str>) {
            match s {
                None => self.varint(0),
                Some(s) => {
                    let bytes = self.host.bytes(s);
                    self.varint(bytes.len() as u64 + 1);
                    self.out.extend_from_slice(bytes);
                }
            }
        }

        fn function(&mut self, f: &ProtoData<H::Str>, psource: Option<H::Str>) {
            #[cfg(rustynail_lua54)]
            self.string(if self.strip || f.source == psource {
                None
            } else {
                f.source
            });
            #[cfg(not(rustynail_lua54))]
            let _ = psource;
            self.int(f.line_defined);
            self.int(f.last_line_defined);
            self.byte(f.num_params);
            #[cfg(not(rustynail_lua54))]
            self.byte(f.flags & 3); // PF_VAHID | PF_VATAB
            #[cfg(rustynail_lua54)]
            self.byte(u8::from(f.flags & 3 != 0));
            self.byte(f.max_stack_size);
            // code
            self.int(f.code.len() as i32);
            #[cfg(not(rustynail_lua54))]
            self.align(4);
            for &i in &f.code {
                self.out.extend_from_slice(&i.to_le_bytes());
            }
            // constants
            self.int(f.k.len() as i32);
            for k in &f.k {
                match *k {
                    Const::Nil => self.byte(LUA_VNIL),
                    Const::False => self.byte(LUA_VFALSE),
                    Const::True => self.byte(LUA_VTRUE),
                    Const::Float(x) => {
                        self.byte(LUA_VNUMFLT);
                        self.out.extend_from_slice(&x.to_le_bytes());
                    }
                    Const::Int(i) => {
                        self.byte(LUA_VNUMINT);
                        self.integer(i);
                    }
                    Const::Str(s) => {
                        let tag = if self.host.bytes(s).len() <= 40 {
                            LUA_VSHRSTR
                        } else {
                            LUA_VLNGSTR
                        };
                        self.byte(tag);
                        self.string(Some(s));
                    }
                }
            }
            // upvalues
            self.int(f.upvalues.len() as i32);
            for u in &f.upvalues {
                self.byte(u8::from(u.in_stack));
                self.byte(u.idx);
                self.byte(u.kind);
            }
            // protos
            self.int(f.protos.len() as i32);
            for p in &f.protos {
                self.function(p, f.source);
            }
            #[cfg(not(rustynail_lua54))]
            self.string(if self.strip { None } else { f.source });
            // debug
            let n = if self.strip { 0 } else { f.line_info.len() };
            self.int(n as i32);
            self.out.extend(f.line_info[..n].iter().map(|&b| b as u8));
            let n = if self.strip { 0 } else { f.abs_line_info.len() };
            self.int(n as i32);
            #[cfg(not(rustynail_lua54))]
            if n > 0 {
                self.align(4);
                for a in &f.abs_line_info[..n] {
                    self.out.extend_from_slice(&a.pc.to_le_bytes());
                    self.out.extend_from_slice(&a.line.to_le_bytes());
                }
            }
            #[cfg(rustynail_lua54)]
            for a in &f.abs_line_info[..n] {
                self.int(a.pc as i32);
                self.int(a.line);
            }
            let n = if self.strip { 0 } else { f.loc_vars.len() };
            self.int(n as i32);
            for v in &f.loc_vars[..n] {
                self.string(Some(v.name));
                self.int(v.start_pc as i32);
                self.int(v.end_pc as i32);
            }
            let n = if self.strip { 0 } else { f.upvalues.len() };
            self.int(n as i32);
            for u in &f.upvalues[..n] {
                self.string(u.name);
            }
        }
    }

    pub fn dump<H: CompilerHost>(p: &ProtoData<H::Str>, host: &H, strip: bool) -> Vec<u8> {
        let mut d = Dumper {
            host,
            out: Vec::new(),
            strip,
            #[cfg(not(rustynail_lua54))]
            seen: Default::default(),
            #[cfg(not(rustynail_lua54))]
            nstr: 0,
        };
        d.out.extend_from_slice(LUA_SIGNATURE);
        d.byte(LUAC_VERSION);
        d.byte(LUAC_FORMAT);
        d.out.extend_from_slice(LUAC_DATA);
        #[cfg(not(rustynail_lua54))]
        {
            d.byte(4);
            d.out.extend_from_slice(&(LUAC_INT as i32).to_le_bytes());
            d.byte(4);
            d.out.extend_from_slice(&LUAC_INST.to_le_bytes());
            d.byte(8);
            d.out.extend_from_slice(&LUAC_INT.to_le_bytes());
            d.byte(8);
            d.out.extend_from_slice(&LUAC_NUM.to_le_bytes());
        }
        #[cfg(rustynail_lua54)]
        {
            d.byte(4);
            d.byte(8);
            d.byte(8);
            d.out.extend_from_slice(&LUAC_INT.to_le_bytes());
            d.out.extend_from_slice(&LUAC_NUM.to_le_bytes());
        }
        d.byte(p.upvalues.len() as u8);
        d.function(p, None);
        d.out
    }
}

/// Serialize `p` as a precompiled chunk (`string.dump` / `luac -o`). With `strip`, debug information
/// is omitted exactly as in `luaU_dump`.
pub fn dump<H: CompilerHost>(p: &ProtoData<H::Str>, host: &H, strip: bool) -> Vec<u8> {
    imp::dump(p, host, strip)
}
