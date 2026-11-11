//! Structure-aware Lua program generator for the JIT differential target: the input bytes drive the
//! choices of a grammar (loops, arithmetic, tables, calls, closures, varargs, pcall, coroutines,
//! metamethods, to-be-closed variables). The output is almost always a syntactically valid program;
//! runtime errors are expected and are part of what is compared. When the bytes run out every choice
//! becomes 0 (the simplest alternative), so generation always terminates.

const INTS: &[&str] = &[
    "0", "1", "2", "3", "7", "-1", "-2", "100", "255", "64", "63", "math.maxinteger",
    "math.mininteger", "(1<<62)", "0x7fffffff", "(-0x80000000)", "(1<<53)", "((1<<53)+1)",
];
const FLTS: &[&str] = &[
    "0.0", "(-0.0)", "0.5", "1.5", "(-2.5)", "3.0", "1e308", "(-1e308)", "(0/0)", "(1/0)", "(-1/0)",
    "2^53", "2^63", "(-2^63)", "1e15", "1e-310",
];
const STRS: &[&str] = &[
    "''", "'10'", "'0x10'", "'abc'", "' 5 '", "'1e2'", "'1'", "'a\\0b'", "'-3'", "'x'",
];
const OTHERS: &[&str] = &["nil", "true", "false", "nil"];
const ARITH: &[&str] = &["+", "-", "*", "/", "//", "%", "^"];
const BITS: &[&str] = &["&", "|", "~", "<<", ">>"];
const CMPS: &[&str] = &["<", "<=", ">", ">=", "==", "~="];
const EVENTS: &[&str] = &[
    "__add", "__sub", "__mul", "__div", "__mod", "__idiv", "__pow", "__unm", "__band", "__bor",
    "__shl", "__bnot", "__concat", "__len", "__eq", "__lt", "__le", "__index", "__newindex",
    "__call",
];
const LIBFN: &[&str] = &[
    "tostring", "tonumber", "math.type", "math.floor", "math.abs", "math.tointeger", "math.ult",
    "math.max", "math.min", "math.fmod", "type", "rawlen", "rawequal", "string.len", "string.upper",
    "string.byte", "string.char", "string.sub", "select", "rawget", "utf8.char",
];

const KV: u8 = 0; // any value
const KF: u8 = 1; // function
const KT: u8 = 2; // table
const KRO: u8 = 3; // read-only (loop control, tbc)

pub struct Gen<'a> {
    d: &'a [u8],
    p: usize,
    pub out: String,
    sc: Vec<(String, u8)>,
    id: u32,
    fuel: i32,
    loops: Vec<Option<String>>,
    vararg: Vec<bool>,
    dbg: bool,
    force: bool,
}

impl<'a> Gen<'a> {
    pub fn new(d: &'a [u8], dbg: bool) -> Self {
        Gen { d, p: 0, out: String::new(), sc: vec![], id: 100, fuel: 70, loops: vec![], vararg: vec![false], dbg, force: false }
    }

    fn n(&mut self, m: u32) -> u32 {
        let b = if self.p < self.d.len() {
            self.p += 1;
            self.d[self.p - 1] as u32
        } else {
            0
        };
        b % m
    }
    fn stop(&self) -> bool {
        self.p >= self.d.len() || self.fuel <= 0 || self.out.len() > 9000
    }
    fn w(&mut self, s: &str) {
        self.out.push_str(s);
    }
    fn nl(&mut self) {
        let c = if self.n(4) == 0 { ' ' } else { '\n' };
        self.out.push(c);
    }
    fn name(&mut self, pre: &str) -> String {
        self.id += 1;
        format!("{}{}", pre, self.id)
    }
    fn pick<T: Copy>(&mut self, t: &[T]) -> T {
        let i = self.n(t.len() as u32) as usize;
        t[i]
    }
    fn var(&mut self, kind: Option<u8>) -> Option<String> {
        let c: Vec<usize> = (0..self.sc.len())
            .filter(|&i| kind.map_or(true, |k| self.sc[i].1 == k))
            .collect();
        if c.is_empty() {
            return None;
        }
        let i = c[self.n(c.len() as u32) as usize];
        Some(self.sc[i].0.clone())
    }
    fn var_or_const(&mut self, kind: Option<u8>) {
        match self.var(kind) {
            Some(v) => self.w(&v),
            None => self.konst(),
        }
    }
    fn table_var(&mut self) -> String {
        self.var(Some(KT)).unwrap_or_else(|| "{}".into())
    }

    fn konst(&mut self) {
        match self.n(12) {
            0..=2 => {
                let s = self.pick(INTS);
                self.w(s)
            }
            3..=4 => {
                let v = self.n(30);
                self.w(&v.to_string())
            }
            5..=6 => {
                let s = self.pick(FLTS);
                self.w(s)
            }
            7 => {
                let v = self.n(200) as f64 / 8.0;
                self.w(&format!("{v:?}"))
            }
            8..=9 => {
                let s = self.pick(STRS);
                self.w(s)
            }
            _ => {
                let s = self.pick(OTHERS);
                self.w(s)
            }
        }
    }

    fn args(&mut self, d: u32) {
        let k = self.n(4);
        for i in 0..k {
            if i > 0 {
                self.w(", ");
            }
            if i == k - 1 && self.vararg.last() == Some(&true) && self.n(4) == 0 {
                self.w("...");
            } else {
                self.expr(d + 1);
            }
        }
    }

    pub fn expr(&mut self, d: u32) {
        let c = if d >= 3 || self.stop() { self.n(3) } else { self.n(20) };
        match c {
            0 | 1 => self.konst(),
            2 => {
                if self.n(6) == 0 {
                    self.var_or_const(None)
                } else {
                    self.scalar()
                }
            }
            3..=5 => self.bin(d, ARITH),
            6 => self.bin(d, BITS),
            7 => self.bin(d, CMPS),
            8 => self.bin(d, &["and", "or"]),
            9 => self.bin(d, &[".."]),
            10 => {
                let op = self.pick(&["-", "not ", "#", "~"]);
                self.w("(");
                self.w(op);
                self.expr(d + 1);
                self.w(")");
            }
            11 => match self.n(3) {
                0 => {
                    let t = self.table_var();
                    self.w(&t);
                    self.w("[");
                    self.expr(d + 1);
                    self.w("]");
                }
                1 => {
                    let t = self.table_var();
                    self.w(&t);
                    let f = self.pick(&[".x", ".y", ".n"]);
                    self.w(f);
                }
                _ => {
                    self.w("(");
                    self.expr(d + 1);
                    self.w(")[");
                    self.expr(d + 1);
                    self.w("]");
                }
            },
            12 | 13 => {
                self.w("(");
                self.call(d);
                self.w(")");
            }
            14 => self.table_cons(d),
            15 => self.closure(d, 2),
            16 => {
                let f = self.pick(LIBFN);
                self.w(f);
                self.w("(");
                self.args(d);
                self.w(")");
            }
            17 => {
                if self.vararg.last() == Some(&true) {
                    let f = self.pick(&["select('#', ...)", "(select(2, ...))", "(...)", "select(-1, ...)"]);
                    self.w(f);
                } else {
                    self.konst();
                }
            }
            18 => {
                self.w("(pcall(");
                self.var_or_const(Some(KF));
                self.w(", ");
                self.args(d);
                self.w("))");
            }
            _ => {
                let f = self.pick(&[
                    "string.format('%d %s %g', ",
                    "string.format('%5.2f|%x', ",
                    "string.format('%q', ",
                    "table.concat({",
                ]);
                self.w(f);
                self.expr(d + 1);
                self.w(", ");
                self.expr(d + 1);
                if f.starts_with("table") {
                    self.w("}, ',')");
                } else {
                    self.w(")");
                }
            }
        }
    }

    fn bin(&mut self, d: u32, ops: &[&str]) {
        let op = self.pick(ops);
        self.w("(");
        self.operand(d, op);
        self.w(" ");
        self.w(op);
        self.w(" ");
        self.operand(d, op);
        self.w(")");
    }

    /// Operand of a binary operator: mostly plain scalars (so that programs get past the first
    /// type error), sometimes any expression.
    fn operand(&mut self, d: u32, op: &str) {
        if matches!(op, "and" | "or" | "..") || self.n(8) >= 6 {
            return self.expr(d + 1);
        }
        match self.n(4) {
            0 => {
                let s = self.pick(INTS);
                self.w(s)
            }
            1 => {
                let v = self.n(30);
                self.w(&v.to_string())
            }
            _ => self.scalar(),
        }
    }

    fn scalar(&mut self) {
        let k = if self.n(2) == 0 { KV } else { KRO };
        match self.var(Some(k)).or_else(|| self.var(Some(KV + KRO - k))) {
            Some(v) => self.w(&v),
            None => self.konst(),
        }
    }

    fn call(&mut self, d: u32) {
        if self.n(5) == 0 {
            let t = self.table_var();
            self.w(&t);
            let m = self.pick(&[":m1(", ":m2(", ":m3("]);
            self.w(m);
        } else {
            self.var_or_const(Some(KF));
            self.w("(");
        }
        self.args(d);
        self.w(")");
    }

    fn table_cons(&mut self, d: u32) {
        self.w("{");
        let k = self.n(5);
        for _ in 0..k {
            match self.n(4) {
                0 | 1 => self.expr(d + 1),
                2 => {
                    let f = self.pick(&["x", "y", "n"]);
                    self.w(f);
                    self.w(" = ");
                    self.expr(d + 1);
                }
                _ => {
                    self.w("[");
                    self.expr(d + 1);
                    self.w("] = ");
                    self.expr(d + 1);
                }
            }
            self.w(", ");
        }
        if self.vararg.last() == Some(&true) && self.n(4) == 0 {
            self.w("...");
        }
        self.w("}");
    }

    /// `function(params) <body> end` (`np` max params); the body sees the enclosing locals.
    fn closure(&mut self, d: u32, np: u32) {
        let params = self.n(np + 1);
        self.func_literal(d, params, None);
    }

    fn func_literal(&mut self, d: u32, params: u32, names: Option<&[&str]>) {
        let mark = self.sc.len();
        self.w("function(");
        let mut ps = vec![];
        for i in 0..params {
            let nm = match names {
                Some(n) => n[i as usize].to_string(),
                None => self.name("p"),
            };
            if i > 0 {
                self.w(", ");
            }
            self.w(&nm);
            ps.push(nm);
        }
        let va = names.is_none() && self.n(4) == 0;
        if va {
            if params > 0 {
                self.w(", ");
            }
            self.w("...");
        }
        self.w(")\n");
        self.tick();
        for p in ps {
            self.sc.push((p, KV));
        }
        let saved = std::mem::take(&mut self.loops);
        self.vararg.push(va);
        self.stmts(d + 1, 3);
        match self.n(4) {
            0 => {}
            1 => {
                self.w("do return ");
                self.expr(d + 1);
                self.w(" end\n");
            }
            2 => {
                self.w("do return ");
                self.call(d);
                self.w(" end\n"); // tail call
            }
            _ => {
                self.w("do return ");
                self.expr(d + 1);
                self.w(", ");
                self.expr(d + 1);
                self.w(" end\n");
            }
        }
        self.vararg.pop();
        self.loops = saved;
        self.sc.truncate(mark);
        self.w("end");
    }

    fn stmts(&mut self, d: u32, maxn: u32) {
        let mark = self.sc.len();
        let k = 1 + self.n(maxn);
        for _ in 0..k {
            if self.stop() {
                break;
            }
            self.stmt(d);
        }
        self.sc.truncate(mark);
    }

    fn block(&mut self, d: u32) {
        self.stmts(d, 3);
    }

    /// `tick(site)`: the cooperative budget (see `lib.rs`).
    fn tick(&mut self) {
        self.id += 1;
        let site = self.id;
        self.w(&format!("tick({site}) "));
    }

    fn loop_body(&mut self, d: u32, label: bool) {
        let force = std::mem::take(&mut self.force);
        // nested loops always tick: constant bounds (14 x 14 x 14 x ...) multiply into minutes under ASan
        if force || self.n(2) == 0 || !self.loops.is_empty() {
            self.tick();
        }
        let lab = if label { Some(self.name("c")) } else { None };
        self.loops.push(lab.clone());
        if self.n(8) == 0 {
            self.w("jit.flush()\n");
        }
        self.block(d + 1);
        self.loops.pop();
        if let Some(l) = lab {
            self.w(&format!("::{l}::\n"));
        }
    }

    fn cond(&mut self, d: u32) {
        if self.n(3) == 0 {
            self.expr(d + 1);
        } else {
            self.bin(d, CMPS);
        }
    }

    fn emit_stmt(&mut self, d: u32) {
        self.w("emit(");
        let k = 1 + self.n(3);
        for i in 0..k {
            if i > 0 {
                self.w(", ");
            }
            self.expr(d + 1);
        }
        self.w(")");
    }

    fn stmt(&mut self, d: u32) {
        self.fuel -= 1;
        let c = if d >= 4 {
            self.n(6)
        } else if self.dbg {
            self.n(34)
        } else {
            self.n(31)
        };
        match c {
            0 | 1 => {
                let v = self.name("v");
                self.w(&format!("local {v} = "));
                self.expr(d);
                self.sc.push((v, KV));
            }
            2 | 3 => {
                if let Some(v) = self.var(Some(KV)) {
                    self.w(&format!("{v} = "));
                    self.expr(d);
                } else {
                    self.emit_stmt(d);
                }
            }
            4 => {
                let t = self.table_var();
                match self.n(3) {
                    0 => {
                        self.w(&format!("{t}["));
                        self.expr(d + 1);
                        self.w("]");
                    }
                    1 => {
                        let f = self.pick(&[".x", ".y", ".n"]);
                        self.w(&format!("{t}{f}"));
                    }
                    _ => self.w(&format!("{t}[#{t} + 1]")),
                }
                self.w(" = ");
                self.expr(d);
            }
            5 => self.emit_stmt(d),
            6 | 7 => {
                self.w("if ");
                self.cond(d);
                self.w(" then\n");
                self.block(d + 1);
                if self.n(2) == 0 {
                    self.w("else\n");
                    self.block(d + 1);
                }
                self.w("end");
            }
            8..=10 => {
                // numeric for
                let v = self.name("i");
                self.w(&format!("for {v} = "));
                match self.n(6) {
                    0 => {
                        // a huge constant start (-0x80000000 ..) with a small bound is 2^31 iterations: tick it
                        self.force = true;
                        self.konst()
                    }
                    1 => self.w("1.0"),
                    _ => {
                        let a = self.n(3);
                        self.w(&a.to_string())
                    }
                }
                self.w(", ");
                if self.n(6) == 0 {
                    self.force = true;
                    self.expr(d + 1);
                } else {
                    let b = 1 + self.n(40);
                    self.w(&b.to_string());
                }
                match self.n(8) {
                    0..=3 => {}
                    4 => self.w(", -1"),
                    5 => self.w(", 2"),
                    6 => {
                        self.force = true;
                        self.w(", ");
                        let s = self.pick(&["0.5", "0", "3", "-0.25", "'1'", "math.maxinteger"]);
                        self.w(s)
                    }
                    _ => {
                        self.force = true;
                        self.w(", ");
                        self.expr(d + 1);
                    }
                }
                self.w(" do\n");
                self.sc.push((v, KRO));
                self.loop_body(d, true);
                self.sc.pop();
                self.w("end");
            }
            11 | 12 => {
                // generic for
                let (k, v) = (self.name("k"), self.name("e"));
                self.w(&format!("for {k}, {v} in "));
                let kind = self.n(4);
                match kind {
                    0 => self.w("pairs("),
                    1 | 2 => self.w("ipairs("),
                    _ => self.w("next, "),
                }
                // assigning to a new field during a pairs/next traversal is undefined behaviour in
                // Lua, and the traversal order of address-keyed (function/table) entries depends on the
                // addresses: traverse a sorted snapshot of the scalar keys (ipairs may use the live table)
                let snap = kind == 0 || kind == 3;
                if snap {
                    self.w("snap(");
                }
                if self.n(4) == 0 {
                    self.table_cons(d);
                } else {
                    let t = self.table_var();
                    self.w(&t);
                }
                if snap {
                    self.w(")");
                }
                if kind < 3 {
                    self.w(")");
                }
                self.w(" do\n");
                self.force = true;
                self.sc.push((k, KRO));
                self.sc.push((v, KV));
                self.loop_body(d, true);
                self.sc.pop();
                self.sc.pop();
                self.w("end");
            }
            13 => {
                let cn = self.name("w");
                let k = 1 + self.n(30);
                self.w(&format!("local {cn} = 0\nwhile {cn} < {k} and "));
                self.cond(d);
                self.w(&format!(" do\n{cn} = {cn} + 1\n"));
                self.sc.push((cn, KRO));
                self.loop_body(d, true);
                self.sc.pop();
                self.w("end");
            }
            14 => {
                let cn = self.name("w");
                let k = 1 + self.n(30);
                self.w(&format!("local {cn} = 0\nrepeat\n{cn} = {cn} + 1\n"));
                if !self.loops.is_empty() || self.n(2) == 0 {
                    self.tick(); // as loop_body: nested constant-bound loops multiply
                }
                let mark = self.sc.len();
                self.loops.push(None);
                self.block(d + 1);
                self.loops.pop();
                self.w(&format!("until {cn} >= {k} or "));
                self.cond(d);
                self.sc.truncate(mark);
            }
            15 => {
                let f = self.name("f");
                self.w(&format!("local function {f}"));
                let params = self.n(4);
                let mut tmp = String::new();
                std::mem::swap(&mut tmp, &mut self.out);
                self.func_literal(d, params, None);
                std::mem::swap(&mut tmp, &mut self.out);
                self.out.push_str(&tmp["function".len()..]);
                self.sc.push((f, KF));
            }
            16 => {
                let f = self.name("f");
                self.w(&format!("local {f} = "));
                self.closure(d, 3);
                self.sc.push((f, KF));
            }
            17 | 18 => {
                if self.n(2) == 0 {
                    self.w("emit(");
                    self.call(d);
                    self.w(")");
                } else {
                    self.call(d);
                }
            }
            19 => {
                self.w("emit(");
                if self.n(3) == 0 {
                    self.w("xpcall(");
                    self.var_or_const(Some(KF));
                    self.w(", function(m) return m end, ");
                } else {
                    self.w("pcall(");
                    self.var_or_const(Some(KF));
                    self.w(", ");
                }
                self.args(d);
                self.w("))");
            }
            20 => {
                self.w("error(");
                self.expr(d + 1);
                if self.n(3) == 0 {
                    let l = self.pick(&[", 0", ", 1", ", 2"]);
                    self.w(l);
                }
                self.w(")");
            }
            21 => {
                let t = self.name("t");
                self.w(&format!("local {t} = "));
                self.table_cons(d);
                self.sc.push((t.clone(), KT));
                if self.n(2) == 0 {
                    let k = 1 + self.n(40);
                    let i = self.name("i");
                    self.w(&format!("\nfor {i} = 1, {k} do {t}[{i}] = "));
                    self.sc.push((i, KRO));
                    self.expr(d + 1);
                    self.sc.pop();
                    self.w(" end");
                }
            }
            22 => {
                let Some(t) = self.var(Some(KT)) else {
                    self.emit_stmt(d);
                    self.nl();
                    return;
                };
                self.w(&format!("setmetatable({t}, {{"));
                let k = 1 + self.n(3);
                for _ in 0..k {
                    let ev = self.pick(EVENTS);
                    self.w(&format!("{ev} = "));
                    if (ev == "__index" || ev == "__newindex") && self.n(3) == 0 {
                        let t = self.table_var();
                        self.w(&t);
                    } else if ev == "__newindex" {
                        self.func_literal(d + 1, 3, Some(&["a", "b", "c"]));
                    } else {
                        self.func_literal(d + 1, 2, Some(&["a", "b"]));
                    }
                    self.w(", ");
                }
                self.w("})");
            }
            23 => {
                let co = self.name("co");
                if self.n(2) == 0 {
                    self.w(&format!("local {co} = coroutine.wrap("));
                    self.closure(d + 1, 2);
                    self.w(")\n");
                    let k = 1 + self.n(3);
                    for _ in 0..k {
                        self.w(&format!("emit(pcall({co}, "));
                        self.args(d);
                        self.w("))\n");
                    }
                    self.sc.push((co, KF));
                } else {
                    self.w(&format!("local {co} = coroutine.create("));
                    self.closure(d + 1, 2);
                    self.w(")\n");
                    let k = 1 + self.n(3);
                    for _ in 0..k {
                        self.w(&format!("emit(coroutine.resume({co}, "));
                        self.args(d);
                        self.w("))\n");
                    }
                    if self.n(2) == 0 {
                        self.w(&format!("emit(coroutine.status({co}), coroutine.close({co}))"));
                    } else {
                        self.w(&format!("emit(coroutine.status({co}))"));
                    }
                }
            }
            24 => {
                // yield (an error outside coroutines, which is compared too)
                self.w("emit(coroutine.yield(");
                self.args(d);
                self.w("))");
            }
            25 => {
                let x = self.name("x");
                self.w(&format!(
                    "do\nlocal {x} <close> = setmetatable({{}}, {{__close = function(o, e) emit('close', e) end}})\n"
                ));
                self.sc.push((x, KRO));
                self.block(d + 1);
                self.sc.pop();
                self.w("end");
            }
            26 => {
                let t = self.table_var();
                match self.n(6) {
                    0 => {
                        self.w(&format!("table.insert({t}, "));
                        self.expr(d + 1);
                        self.w(")");
                    }
                    1 => {
                        self.w(&format!("emit(table.remove({t}"));
                        if self.n(3) == 0 {
                            self.w(", ");
                            self.expr(d + 1);
                        }
                        self.w("))");
                    }
                    2 => {
                        self.w(&format!("table.sort({t}"));
                        if self.n(2) == 0 {
                            self.w(", function(a, b) return a < b end");
                        }
                        self.w(")");
                    }
                    3 => self.w(&format!("emit(table.unpack({t}))")),
                    4 => self.w(&format!("emit(table.concat({t}, '-'))")),
                    _ => {
                        self.w(&format!("table.insert({t}, 1, "));
                        self.expr(d + 1);
                        self.w(")");
                    }
                }
            }
            27 => {
                let r = self.name("r");
                self.w(&format!("local function {r}(n, acc)\n"));
                self.tick();
                self.w("if n <= 0 then return acc end\n");
                let mark = self.sc.len();
                self.sc.push(("n".into(), KRO));
                self.sc.push(("acc".into(), KV));
                if self.n(2) == 0 {
                    self.w(&format!("return {r}(n - 1, acc + "));
                    self.expr(d + 2);
                    self.w(")\n");
                } else {
                    self.w(&format!("return n + {r}(n - 1, acc)\n"));
                }
                self.sc.truncate(mark);
                let k = self.n(120);
                self.w(&format!("end\nemit(pcall({r}, {k}, 0))"));
            }
            28 => {
                if let Some(t) = self.var(Some(KT)) {
                    let m = self.pick(&["m1", "m2", "m3"]);
                    let p = self.name("p");
                    self.w(&format!("function {t}:{m}({p})\n"));
                    self.tick();
                    let mark = self.sc.len();
                    self.sc.push(("self".into(), KT));
                    self.sc.push((p, KV));
                    let saved = std::mem::take(&mut self.loops);
                    self.vararg.push(false);
                    self.block(d + 1);
                    self.w("do return ");
                    self.expr(d + 1);
                    self.w(" end\n");
                    self.vararg.pop();
                    self.loops = saved;
                    self.sc.truncate(mark);
                    self.w("end");
                } else {
                    self.emit_stmt(d);
                }
            }
            29 => {
                let tgt = self.loops.last().cloned();
                match (self.n(3), tgt) {
                    (0, Some(_)) => {
                        self.w("if ");
                        self.cond(d + 1);
                        self.w(" then break end");
                    }
                    (1, Some(Some(l))) => {
                        self.w("if ");
                        self.cond(d + 1);
                        self.w(&format!(" then goto {l} end"));
                    }
                    (2, _) => {
                        self.w("if ");
                        self.cond(d + 1);
                        self.w(" then return ");
                        self.expr(d + 1);
                        self.w(" end");
                    }
                    _ => self.emit_stmt(d),
                }
            }
            31 => {
                // hook installed around a block (compiled code is entered only while no hook is set)
                let mask = self.pick(&["'', 3", "'', 1", "'l'", "'c'", "'r'", "'crl'", "'', 17"]);
                self.w(&format!("debug.sethook(function(e, l) HC = (HC or 0) + 1 end, {mask})\n"));
                self.block(d + 1);
                self.w("\ndebug.sethook()\nemit(HC)");
            }
            32 => {
                let f = self.pick(&[
                    "debug.traceback('tb')",
                    "debug.getinfo(1, 'l').currentline",
                    "debug.getinfo(2, 'l').currentline",
                    "debug.getinfo(1, 'n').name",
                    "debug.getinfo(1, 'S').linedefined",
                    "debug.getlocal(1, 1)",
                    "debug.getlocal(1, 2)",
                    "debug.getinfo(1, 'u').nparams",
                ]);
                self.w(&format!("emit({f})"));
            }
            33 => {
                // (no setlocal: overwriting a loop counter or hidden for-state slot makes loops endless)
                self.w("emit(debug.getlocal(1, 3), debug.getlocal(2, 1))");
            }
            _ if self.n(2) == 0 => {
                // jit.flush() in the middle of whatever is running (a semantic no-op in both modes)
                self.w("jit.flush()");
            }
            _ => {
                self.w("do\n");
                self.block(d + 1);
                self.w("end");
            }
        }
        self.nl();
        if self.n(4) == 0 {
            self.w("emit(");
            self.scalar();
            self.w(", ");
            self.scalar();
            self.w(")\n");
        }
    }

    /// A whole program: shared locals, then independent segments, each run under `pcall`.
    pub fn program(&mut self) {
        self.w("local t1 = {1, 2, 3, x = 4}\nlocal t2 = {}\nlocal v1, v2 = 1, 2.5\nlocal function f1(a, b) return a, b end\nfunction t1:m1(p) return p end\nfunction t1:m2(p) return self[1], p end\nfunction t2:m3(p) return self.n, p end\nlocal function snap(t) local ks = {} for k in next, t do local ty = type(k) if ty == 'number' or ty == 'string' or ty == 'boolean' then ks[#ks + 1] = k end end table.sort(ks, function(a, b) local ta, tb = type(a), type(b) if ta ~= tb then return ta < tb end if ta == 'boolean' then return (a and 1 or 0) < (b and 1 or 0) end return a < b end) local c = {} for _, k in ipairs(ks) do c[k] = rawget(t, k) end return c end\n");
        self.sc.push(("t1".into(), KT));
        self.sc.push(("t2".into(), KT));
        self.sc.push(("v1".into(), KV));
        self.sc.push(("v2".into(), KV));
        self.sc.push(("f1".into(), KF));
        let segs = 1 + self.n(5);
        for _ in 0..segs {
            if self.stop() {
                break;
            }
            self.w("emit(pcall(function()\n");
            self.stmts(0, 6);
            self.w("end))\n");
        }
        self.w("emit(v1, v2, #t1, #t2, t1.x)\n");
    }
}

/// The program for `data`.
pub fn generate(data: &[u8], dbg: bool) -> String {
    let mut g = Gen::new(data, dbg);
    g.program();
    g.out
}
