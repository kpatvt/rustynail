//! Bytecode listing: port of the `print.c` section of the reference `luac.c` (`luac -l [-l]`).
use rn_core::host::CompilerHost;
use rn_core::instruction::{Instruction, MAXARG_C};
use rn_core::number::fmt_g;
use rn_core::protodata::{Const, ProtoData};
use rn_core::ver::active::{OpCode, OP_NAMES};
use std::fmt::Write;

/// Listing options.
#[derive(Clone, Copy, Debug, Default)]
pub struct ListOpts {
    /// `luac -l -l`: also print constants, locals and upvalues.
    pub full: bool,
    /// Print every address as `0xX` (instead of a deterministic fake address) so that listings
    /// can be compared with the reference after the same normalization.
    pub normalize_addresses: bool,
}

const ABSLINEINFO: i8 = -0x80;
const MAXIWTHABS: i32 = 128;

/// `luaG_getfuncline`
pub fn get_func_line<S>(f: &ProtoData<S>, pc: usize) -> i32 {
    if f.line_info.is_empty() {
        return -1;
    }
    let pc_i = pc as i32;
    let (mut basepc, mut baseline) =
        if f.abs_line_info.is_empty() || pc_i < f.abs_line_info[0].pc as i32 {
            (-1i32, f.line_defined)
        } else {
            let mut i = pc_i / MAXIWTHABS - 1;
            if i < 0 {
                i = 0;
            }
            let mut i = i as usize;
            while i + 1 < f.abs_line_info.len() && pc_i >= f.abs_line_info[i + 1].pc as i32 {
                i += 1;
            }
            (f.abs_line_info[i].pc as i32, f.abs_line_info[i].line)
        };
    while basepc < pc_i {
        basepc += 1;
        let d = f.line_info.get(basepc as usize).copied().unwrap_or(0);
        if d != ABSLINEINFO {
            baseline += i32::from(d);
        }
    }
    baseline
}

const EVENT_NAMES: [&str; 25] = [
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

fn eventname(i: u32) -> &'static str {
    EVENT_NAMES.get(i as usize).copied().unwrap_or("?")
}

struct Printer<'a, H: CompilerHost> {
    host: &'a H,
    opts: ListOpts,
    out: String,
    next_addr: usize,
}

impl<H: CompilerHost> Printer<'_, H> {
    fn print_string(&mut self, s: &[u8]) {
        self.out.push('"');
        for &c in s {
            match c {
                b'"' => self.out.push_str("\\\""),
                b'\\' => self.out.push_str("\\\\"),
                7 => self.out.push_str("\\a"),
                8 => self.out.push_str("\\b"),
                12 => self.out.push_str("\\f"),
                b'\n' => self.out.push_str("\\n"),
                b'\r' => self.out.push_str("\\r"),
                b'\t' => self.out.push_str("\\t"),
                11 => self.out.push_str("\\v"),
                32..=126 => self.out.push(c as char),
                _ => {
                    let _ = write!(self.out, "\\{c:03}");
                }
            }
        }
        self.out.push('"');
    }

    fn constant(&mut self, f: &ProtoData<H::Str>, i: usize) {
        match f.k.get(i) {
            None => self.out.push('?'),
            Some(Const::Nil) => self.out.push_str("nil"),
            Some(Const::False) => self.out.push_str("false"),
            Some(Const::True) => self.out.push_str("true"),
            Some(Const::Float(x)) => {
                let b = fmt_g(*x, if cfg!(rustynail_lua54) { 14 } else { 15 });
                let s = String::from_utf8_lossy(b.as_bytes()).into_owned();
                self.out.push_str(&s);
                if s.bytes().all(|c| c == b'-' || c.is_ascii_digit()) {
                    self.out.push_str(".0");
                }
            }
            Some(Const::Int(i)) => {
                let _ = write!(self.out, "{i}");
            }
            Some(Const::Str(s)) => {
                let b = self.host.bytes(*s).to_vec();
                self.print_string(&b);
            }
        }
    }

    fn const_type(&mut self, f: &ProtoData<H::Str>, i: usize) {
        self.out.push_str(match f.k[i] {
            Const::Nil => "N",
            Const::False | Const::True => "B",
            Const::Float(_) => "F",
            Const::Int(_) => "I",
            Const::Str(_) => "S",
        });
        self.out.push('\t');
    }

    fn upval_name(&self, f: &ProtoData<H::Str>, i: usize) -> String {
        match f.upvalues.get(i).and_then(|u| u.name) {
            Some(n) => String::from_utf8_lossy(self.host.bytes(n)).into_owned(),
            None => "-".to_string(),
        }
    }

    fn addr(&mut self, id: usize) -> String {
        if self.opts.normalize_addresses {
            "0xX".to_string()
        } else {
            format!("0x{:012x}", 0x5500_0000_0000usize + id * 0x40)
        }
    }

    fn code(&mut self, f: &ProtoData<H::Str>, ids: &[usize]) {
        for (pc, &w) in f.code.iter().enumerate() {
            let i = Instruction(w);
            let o = match i.opcode() {
                Some(o) => o,
                None => {
                    let _ = writeln!(self.out, "\t{}\t[?]\t?\t{:#x}", pc + 1, w);
                    continue;
                }
            };
            let (a, b, c) = (i.a(), i.b(), i.c());
            let (ax, bx) = (i.ax(), i.bx());
            let (sb, sc) = (i.sb(), i.sc());
            #[cfg(not(rustynail_lua54))]
            let (vb, vc) = (i.vb(), i.vc());
            let sbx = i.sbx();
            let isk = i.k();
            let line = get_func_line(f, pc);
            let _ = write!(self.out, "\t{}\t", pc + 1);
            if line > 0 {
                let _ = write!(self.out, "[{line}]\t");
            } else {
                self.out.push_str("[-]\t");
            }
            let _ = write!(self.out, "{:<9}\t", OP_NAMES[o as usize]);
            let kk = if isk != 0 { "k" } else { "" };
            let extra =
                |pcx: usize| -> u32 { f.code.get(pcx + 1).map_or(0, |&w| Instruction(w).ax()) };
            let extraargc = (extra(pc) as i64) * (i64::from(MAXARG_C) + 1);
            const C: &str = "\t; ";
            use OpCode::*;
            match o {
                Move | Unm | Bnot | Not | Len | Concat => {
                    let _ = write!(self.out, "{a} {b}");
                }
                Loadi | Loadf => {
                    let _ = write!(self.out, "{a} {sbx}");
                }
                Loadk => {
                    let _ = write!(self.out, "{a} {bx}{C}");
                    self.constant(f, bx as usize);
                }
                Loadkx => {
                    let _ = write!(self.out, "{a}{C}");
                    self.constant(f, extra(pc) as usize);
                }
                Loadfalse | Lfalseskip | Loadtrue | Close | Tbc | Varargprep => {
                    let _ = write!(self.out, "{a}");
                }
                Loadnil => {
                    let _ = write!(self.out, "{a} {b}{C}{} out", b + 1);
                }
                Getupval | Setupval => {
                    let n = self.upval_name(f, b as usize);
                    let _ = write!(self.out, "{a} {b}{C}{n}");
                }
                Gettabup => {
                    let n = self.upval_name(f, b as usize);
                    let _ = write!(self.out, "{a} {b} {c}{C}{n} ");
                    self.constant(f, c as usize);
                }
                Gettable | Geti => {
                    let _ = write!(self.out, "{a} {b} {c}");
                }
                Getfield => {
                    let _ = write!(self.out, "{a} {b} {c}{C}");
                    self.constant(f, c as usize);
                }
                Settabup => {
                    let n = self.upval_name(f, a as usize);
                    let _ = write!(self.out, "{a} {b} {c}{kk}{C}{n} ");
                    self.constant(f, b as usize);
                    if isk != 0 {
                        self.out.push(' ');
                        self.constant(f, c as usize);
                    }
                }
                Settable | Seti | SelfOp => {
                    let _ = write!(self.out, "{a} {b} {c}{kk}");
                    if isk != 0 {
                        self.out.push_str(C);
                        self.constant(f, c as usize);
                    }
                }
                Setfield => {
                    let _ = write!(self.out, "{a} {b} {c}{kk}{C}");
                    self.constant(f, b as usize);
                    if isk != 0 {
                        self.out.push(' ');
                        self.constant(f, c as usize);
                    }
                }
                #[cfg(rustynail_lua54)]
                Newtable => {
                    let _ = write!(self.out, "{a} {b} {c}{C}{}", i64::from(c) + extraargc);
                }
                #[cfg(not(rustynail_lua54))]
                Newtable => {
                    let _ = write!(
                        self.out,
                        "{a} {vb} {vc}{kk}{C}{}",
                        i64::from(vc) + extraargc
                    );
                }
                Addi | Shli | Shri => {
                    let _ = write!(self.out, "{a} {b} {sc}");
                }
                Addk | Subk | Mulk | Modk | Powk | Divk | Idivk | Bandk | Bork | Bxork => {
                    let _ = write!(self.out, "{a} {b} {c}{C}");
                    self.constant(f, c as usize);
                }
                Add | Sub | Mul | Mod | Pow | Div | Idiv | Band | Bor | Bxor | Shl | Shr => {
                    let _ = write!(self.out, "{a} {b} {c}");
                }
                Mmbin => {
                    let _ = write!(self.out, "{a} {b} {c}{C}{}", eventname(c));
                }
                Mmbini => {
                    let _ = write!(self.out, "{a} {sb} {c} {isk}{C}{}", eventname(c));
                    if isk != 0 {
                        self.out.push_str(" flip");
                    }
                }
                Mmbink => {
                    let _ = write!(self.out, "{a} {b} {c} {isk}{C}{} ", eventname(c));
                    self.constant(f, b as usize);
                    if isk != 0 {
                        self.out.push_str(" flip");
                    }
                }
                Jmp => {
                    let sj = i.sj();
                    let _ = write!(self.out, "{sj}{C}to {}", sj + pc as i32 + 2);
                }
                Eq | Lt | Le => {
                    let _ = write!(self.out, "{a} {b} {isk}");
                }
                Eqk => {
                    let _ = write!(self.out, "{a} {b} {isk}{C}");
                    self.constant(f, b as usize);
                }
                Eqi | Lti | Lei | Gti | Gei => {
                    let _ = write!(self.out, "{a} {sb} {isk}");
                }
                Test => {
                    let _ = write!(self.out, "{a} {isk}");
                }
                Testset => {
                    let _ = write!(self.out, "{a} {b} {isk}");
                }
                Call => {
                    let _ = write!(self.out, "{a} {b} {c}{C}");
                    if b == 0 {
                        self.out.push_str("all in ");
                    } else {
                        let _ = write!(self.out, "{} in ", b - 1);
                    }
                    if c == 0 {
                        self.out.push_str("all out");
                    } else {
                        let _ = write!(self.out, "{} out", c - 1);
                    }
                }
                Tailcall => {
                    let _ = write!(self.out, "{a} {b} {c}{kk}{C}{} in", i64::from(b) - 1);
                }
                Return => {
                    let _ = write!(self.out, "{a} {b} {c}{kk}{C}");
                    if b == 0 {
                        self.out.push_str("all out");
                    } else {
                        let _ = write!(self.out, "{} out", b - 1);
                    }
                }
                Return0 => {}
                Return1 => {
                    let _ = write!(self.out, "{a}");
                }
                Forloop | Tforloop => {
                    let _ = write!(self.out, "{a} {bx}{C}to {}", pc as i64 - i64::from(bx) + 2);
                }
                Forprep => {
                    let _ = write!(
                        self.out,
                        "{a} {bx}{C}exit to {}",
                        pc as i64 + i64::from(bx) + 3
                    );
                }
                Tforprep => {
                    let _ = write!(self.out, "{a} {bx}{C}to {}", pc as i64 + i64::from(bx) + 2);
                }
                Tforcall => {
                    let _ = write!(self.out, "{a} {c}");
                }
                #[cfg(rustynail_lua54)]
                Setlist => {
                    let _ = write!(self.out, "{a} {b} {c}");
                    if isk != 0 {
                        let _ = write!(self.out, "{C}{}", i64::from(c) + extraargc);
                    }
                }
                #[cfg(not(rustynail_lua54))]
                Setlist => {
                    let _ = write!(self.out, "{a} {vb} {vc}{kk}");
                    if isk != 0 {
                        let _ = write!(self.out, "{C}{}", i64::from(c) + extraargc);
                    }
                }
                Closure => {
                    let id = ids.get(bx as usize).copied().unwrap_or(0);
                    let ad = self.addr(id);
                    let _ = write!(self.out, "{a} {bx}{C}{ad}");
                }
                Vararg => {
                    #[cfg(rustynail_lua54)]
                    let _ = write!(self.out, "{a} {c}{C}");
                    #[cfg(not(rustynail_lua54))]
                    let _ = write!(self.out, "{a} {b} {c}{kk}{C}");
                    if c == 0 {
                        self.out.push_str("all out");
                    } else {
                        let _ = write!(self.out, "{} out", c - 1);
                    }
                }
                #[cfg(not(rustynail_lua54))]
                Getvarg => {
                    let _ = write!(self.out, "{a} {b} {c}");
                }
                #[cfg(not(rustynail_lua54))]
                Errnnil => {
                    let _ = write!(self.out, "{a} {bx}{C}");
                    if bx == 0 {
                        self.out.push('?');
                    } else {
                        self.constant(f, bx as usize - 1);
                    }
                }
                Extraarg => {
                    let _ = write!(self.out, "{ax}");
                }
                _ => {}
            }
            self.out.push('\n');
        }
    }

    fn function(&mut self, f: &ProtoData<H::Str>, id: usize, full: bool) {
        // child ids are assigned in preorder: children of this function start at id+1
        let mut ids = Vec::with_capacity(f.protos.len());
        let mut next = id + 1;
        for p in &f.protos {
            ids.push(next);
            next += count_protos(p);
        }
        self.next_addr = next;
        // header
        let src = f.source.map(|s| self.host.bytes(s).to_vec());
        let src: Vec<u8> = src.unwrap_or_else(|| b"=?".to_vec());
        let name: String = match src.first() {
            Some(b'@') | Some(b'=') => String::from_utf8_lossy(&src[1..]).into_owned(),
            Some(0x1b) => "(bstring)".into(),
            _ => "(string)".into(),
        };
        let name = name.split('\0').next().unwrap_or("").to_string();
        let ss = |x: usize| if x == 1 { "" } else { "s" };
        let ad = self.addr(id);
        let _ = writeln!(
            self.out,
            "\n{} <{}:{},{}> ({} instruction{} at {})",
            if f.line_defined == 0 {
                "main"
            } else {
                "function"
            },
            name,
            f.line_defined,
            f.last_line_defined,
            f.code.len(),
            ss(f.code.len()),
            ad
        );
        let _ = write!(
            self.out,
            "{}{} param{}, {} slot{}, {} upvalue{}, ",
            f.num_params,
            if f.is_vararg() { "+" } else { "" },
            ss(f.num_params as usize),
            f.max_stack_size,
            ss(f.max_stack_size as usize),
            f.upvalues.len(),
            ss(f.upvalues.len())
        );
        let _ = writeln!(
            self.out,
            "{} local{}, {} constant{}, {} function{}",
            f.loc_vars.len(),
            ss(f.loc_vars.len()),
            f.k.len(),
            ss(f.k.len()),
            f.protos.len(),
            ss(f.protos.len())
        );
        self.code(f, &ids);
        if full {
            let _ = writeln!(self.out, "constants ({}) for {}:", f.k.len(), ad);
            for i in 0..f.k.len() {
                let _ = write!(self.out, "\t{i}\t");
                self.const_type(f, i);
                self.constant(f, i);
                self.out.push('\n');
            }
            let _ = writeln!(self.out, "locals ({}) for {}:", f.loc_vars.len(), ad);
            for (i, v) in f.loc_vars.iter().enumerate() {
                let n = String::from_utf8_lossy(self.host.bytes(v.name)).into_owned();
                let _ = writeln!(self.out, "\t{i}\t{n}\t{}\t{}", v.start_pc + 1, v.end_pc + 1);
            }
            let _ = writeln!(self.out, "upvalues ({}) for {}:", f.upvalues.len(), ad);
            for (i, u) in f.upvalues.iter().enumerate() {
                let n = self.upval_name(f, i);
                let _ = writeln!(self.out, "\t{i}\t{n}\t{}\t{}", u8::from(u.in_stack), u.idx);
            }
        }
        for (p, &cid) in f.protos.iter().zip(&ids) {
            self.function(p, cid, full);
        }
    }
}

fn count_protos<S>(p: &ProtoData<S>) -> usize {
    1 + p.protos.iter().map(count_protos).sum::<usize>()
}

/// Produce the `luac -l` (or `-l -l` with `opts.full`) text for `p`.
pub fn list<H: CompilerHost>(p: &ProtoData<H::Str>, host: &H, opts: ListOpts) -> String {
    let mut pr = Printer {
        host,
        opts,
        out: String::new(),
        next_addr: 0,
    };
    pr.function(p, 0, opts.full);
    pr.out
}
