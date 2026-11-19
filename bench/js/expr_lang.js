const ROUNDS = Number(process.argv[2]) || 6;
const floor = Math.floor;
// Lua's % has the sign of the divisor
function lmod(a, b) { const r = a % b; return (r !== 0 && (r < 0) !== (b < 0)) ? r + b : r; }

// ---- program generator
let seed = 4242;
function rnd(n) { seed = (seed * 16807) % 2147483647; return seed % n + 1; }
function genProgram() {
  const lines = [];
  lines.push("fn mix(a, b) = (a * 31 + b * 17) % 1009");
  lines.push("fn sq(a) = a * a % 10007");
  lines.push("fn pick(c, a, b) = c * a + (1 - c) * b");
  const names = [];
  for (let i = 1; i <= 24; i++) names[i] = "v" + i;
  for (let i = 1; i <= 24; i++) lines.push(names[i] + " = " + rnd(100));
  function atom(depth) {
    const r = rnd(10);
    if (r <= 3) return String(rnd(50));
    if (r <= 6) return names[rnd(24)];
    if (depth > 0 && r <= 8) return "(" + atom(depth - 1) + " " + ["+", "-", "*"][rnd(3) - 1] + " " + atom(depth - 1) + ")";
    if (depth > 0 && r === 9) return "mix(" + atom(depth - 1) + ", " + atom(depth - 1) + ")";
    return "-" + atom(0);
  }
  for (let i = 1; i <= 160; i++) {
    let e = atom(3);
    for (let k = 1, kn = rnd(3); k <= kn; k++) e = e + " " + ["+", "-", "*", "%"][rnd(4) - 1] + " " + atom(2);
    const n = names[rnd(24)];
    const form = rnd(6);
    if (form === 1) {
      lines.push(n + " = max(" + e + ", " + atom(1) + ") % 1000003");
    } else if (form === 2) {
      lines.push(n + " = pick(" + atom(1) + " < " + atom(1) + ", sq(" + atom(2) + "), " + e + ") % 1000003");
    } else if (form === 3) {
      lines.push(n + " = abs(" + e + " - " + atom(2) + ") % 1000003");
    } else {
      lines.push(n + " = (" + e + ") % 1000003");
    }
  }
  return lines.join("\n");
}

// ---- tokenizer
const RE_WS = /[ \t\r]+/y, RE_NL = /\n/y, RE_NUM = /\d+/y, RE_ID = /[A-Za-z_][A-Za-z0-9_]*/y;
function tokenize(src) {
  const toks = [];
  let pos = 0;
  const len = src.length;
  while (pos < len) {
    RE_WS.lastIndex = pos;
    if (RE_WS.test(src)) {
      pos = RE_WS.lastIndex;
    } else {
      RE_NL.lastIndex = pos;
      if (RE_NL.test(src)) {
        toks.push({ t: "nl" });
        pos = pos + 1;
      } else {
        RE_NUM.lastIndex = pos;
        let m;
        if (RE_NUM.test(src)) {
          toks.push({ t: "num", v: Number(src.substring(pos, RE_NUM.lastIndex)) });
          pos = RE_NUM.lastIndex;
        } else {
          RE_ID.lastIndex = pos;
          if (RE_ID.test(src)) {
            const w = src.substring(pos, RE_ID.lastIndex);
            if (w === "fn") toks.push({ t: "fn" }); else toks.push({ t: "id", v: w });
            pos = RE_ID.lastIndex;
          } else {
            toks.push({ t: src.charAt(pos) });
            pos = pos + 1;
          }
        }
      }
    }
  }
  toks.push({ t: "eof" });
  return toks;
}

// ---- parser (precedence climbing) -> AST of objects
const PREC = { "<": 1, "+": 2, "-": 2, "*": 3, "/": 3, "%": 3 };
class P {
  constructor(toks) { this.toks = toks; this.i = 0; }
  peek() { return this.toks[this.i]; }
  next() { const t = this.toks[this.i]; this.i = this.i + 1; return t; }
  expect(t) {
    const x = this.next();
    if (x.t !== t) throw new Error("expected " + t + " got " + x.t);
    return x;
  }
  primary() {
    const t = this.next();
    if (t.t === "num") return { k: "num", v: t.v };
    if (t.t === "-") return { k: "neg", e: this.primary() };
    if (t.t === "(") {
      const e = this.expr(0);
      this.expect(")");
      return e;
    }
    if (t.t === "id") {
      if (this.peek().t === "(") {
        this.next();
        const args = [];
        if (this.peek().t !== ")") {
          args.push(this.expr(0));
          while (this.peek().t === ",") { this.next(); args.push(this.expr(0)); }
        }
        this.expect(")");
        return { k: "call", name: t.v, args: args };
      }
      return { k: "var", name: t.v };
    }
    throw new Error("unexpected token " + t.t);
  }
  expr(minPrec) {
    let lhs = this.primary();
    while (true) {
      const op = this.peek().t;
      const prec = PREC[op];
      if (prec === undefined || prec < minPrec) break;
      this.next();
      const rhs = this.expr(prec + 1);
      lhs = { k: "bin", op: op, l: lhs, r: rhs };
    }
    return lhs;
  }
}
function parse(src) {
  const p = new P(tokenize(src));
  const prog = { fns: {}, stmts: [] };
  while (p.peek().t !== "eof") {
    if (p.peek().t === "nl") {
      p.next();
    } else if (p.peek().t === "fn") {
      p.next();
      const name = p.expect("id").v;
      p.expect("(");
      const params = [];
      if (p.peek().t !== ")") {
        params.push(p.expect("id").v);
        while (p.peek().t === ",") { p.next(); params.push(p.expect("id").v); }
      }
      p.expect(")");
      p.expect("=");
      prog.fns[name] = { params: params, body: p.expr(0) };
    } else {
      const name = p.expect("id").v;
      p.expect("=");
      prog.stmts.push({ name: name, e: p.expr(0) });
    }
  }
  return prog;
}

// ---- evaluator
const builtin = {
  max: function (a, b) { if (a > b) return a; return b; },
  min: function (a, b) { if (a < b) return a; return b; },
  abs: function (a) { if (a < 0) return -a; return a; },
};
function evalBin(op, a, b) {
  if (op === "+") return a + b;
  else if (op === "-") return a - b;
  else if (op === "*") return lmod(a * b, 1000003);
  else if (op === "<") { if (a < b) return 1; return 0; }
  else if (op === "%") { if (b === 0) return 0; return lmod(a, b); }
  else { if (b === 0) return 0; return floor(a / b); }
}
function evaluate(n, env, prog) {
  const k = n.k;
  if (k === "num") return n.v;
  else if (k === "var") return env[n.name] ?? 0;
  else if (k === "bin") return evalBin(n.op, evaluate(n.l, env, prog), evaluate(n.r, env, prog));
  else if (k === "neg") return -evaluate(n.e, env, prog);
  else {
    const args = n.args;
    const vals = [];
    for (let i = 0; i < args.length; i++) vals[i] = evaluate(args[i], env, prog);
    const b = builtin[n.name];
    if (b) return b(vals[0], vals[1]);
    const f = prog.fns[n.name];
    const fenv = {};
    for (let i = 0; i < f.params.length; i++) fenv[f.params[i]] = vals[i];
    return evaluate(f.body, fenv, prog);
  }
}
function run(prog, input) {
  const env = { v1: input };
  for (let i = 0; i < prog.stmts.length; i++) {
    const s = prog.stmts[i];
    env[s.name] = lmod(evaluate(s.e, env, prog), 1000003);
  }
  let sum = 0;
  for (let i = 1; i <= 24; i++) sum = lmod(sum + (env["v" + i] ?? 0) * i, 1000003);
  return sum;
}

const src = genProgram();
let check = 0, stmts = 0;
for (let r = 1; r <= ROUNDS; r++) {
  const prog = parse(src);
  stmts = prog.stmts.length;
  for (let input = 1; input <= 40; input++) check = lmod(check + run(prog, input * r), 1000003);
}
console.log(`expr_lang\t${src.length}\t${stmts}\t${check}`);
