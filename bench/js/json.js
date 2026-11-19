const N = Number(process.argv[2]) || 35;

// %.14g
function g14(x) {
  let s = x.toPrecision(14);
  if (s.indexOf("e") >= 0) {
    let [m, e] = s.split("e");
    if (m.indexOf(".") >= 0) m = m.replace(/0+$/, "").replace(/\.$/, "");
    const ev = Number(e);
    return m + "e" + (ev < 0 ? "-" : "+") + String(Math.abs(ev)).padStart(2, "0");
  }
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s;
}

function buildDocument() {
  const parts = [];
  let seed = 74755;
  function rnd(n) { seed = (seed * 1309 + 13849) % 65536; return seed % n; }
  parts.push('{"head":{"requestCounter":4,"name":"bench \\"doc\\"","tags":["a","b","c"]},"operations":[');
  for (let i = 1; i <= 40; i++) {
    if (i > 1) parts.push(",");
    // argument evaluation order as in the Lua call
    const a1 = i, a2 = (rnd(2) === 0) ? "true" : "false", a3 = rnd(100), a4 = rnd(1000), a5 = rnd(500) + 1, a6 = rnd(50), a7 = i;
    const a8 = rnd(1000), a9 = rnd(1000), a10 = rnd(1000), a11 = rnd(1000), a12 = rnd(10000), a13 = (rnd(2) === 0) ? "true" : "false";
    parts.push(
      `{"type":"update","id":${a1},"ok":${a2},"ratio":${a3}.${a4},"neg":-${a5},"exp":${a6}e-2,"payload":{"title":"item number ${a7}",` +
      `"values":[${a8},${a9},${a10},${a11}],"nested":{"x":${a12},"y":null,"flag":${a13},"text":"line1\\nline2 \\u0041\\t"}}}`);
  }
  parts.push("]}");
  return parts.join("");
}

const NULL = {};  // singleton for json null
class Parser {
  constructor(text) { this.text = text; this.index = 0; this.cur = null; this.captureBuf = null; }
  read() {
    this.index = this.index + 1;
    const c = this.text.substring(this.index - 1, this.index);
    if (c === "") this.cur = null; else this.cur = c;
  }
  error(msg) { throw new Error("parse error at " + this.index + ": " + msg); }
  skipWhiteSpace() {
    let c = this.cur;
    while (c === " " || c === "\t" || c === "\n" || c === "\r") {
      this.read();
      c = this.cur;
    }
  }
  readChar(ch) {
    if (this.cur !== ch) this.error("expected " + ch);
    this.read();
  }
  readValue() {
    const c = this.cur;
    if (c === "n") return this.readNull();
    else if (c === "t") return this.readTrue();
    else if (c === "f") return this.readFalse();
    else if (c === '"') return this.readString();
    else if (c === "[") return this.readArray();
    else if (c === "{") return this.readObject();
    else if (c === "-" || (c !== null && c >= "0" && c <= "9")) return this.readNumber();
    this.error("value expected");
  }
  readArray() {
    this.read();
    const arr = { kind: "array", items: [] };
    this.skipWhiteSpace();
    if (this.cur === "]") { this.read(); return arr; }
    while (true) {
      this.skipWhiteSpace();
      arr.items.push(this.readValue());
      this.skipWhiteSpace();
      if (this.cur === ",") this.read(); else if (this.cur === "]") { this.read(); return arr; } else this.error("expected , or ]");
    }
  }
  readObject() {
    this.read();
    const obj = { kind: "object", names: [], values: [] };
    this.skipWhiteSpace();
    if (this.cur === "}") { this.read(); return obj; }
    while (true) {
      this.skipWhiteSpace();
      const name = this.readString().value;
      this.skipWhiteSpace();
      this.readChar(":");
      this.skipWhiteSpace();
      obj.names.push(name);
      obj.values.push(this.readValue());
      this.skipWhiteSpace();
      if (this.cur === ",") this.read(); else if (this.cur === "}") { this.read(); return obj; } else this.error("expected , or }");
    }
  }
  readNull() { this.read(); this.readChar("u"); this.readChar("l"); this.readChar("l"); return NULL; }
  readTrue() { this.read(); this.readChar("r"); this.readChar("u"); this.readChar("e"); return { kind: "bool", value: true }; }
  readFalse() { this.read(); this.readChar("a"); this.readChar("l"); this.readChar("s"); this.readChar("e"); return { kind: "bool", value: false }; }
  readString() {
    this.readChar('"');
    const buf = [];
    while (this.cur !== '"') {
      let c = this.cur;
      if (c === null) this.error("unterminated string");
      if (c === "\\") {
        this.read();
        c = this.cur;
        if (c === "n") buf.push("\n");
        else if (c === "t") buf.push("\t");
        else if (c === "r") buf.push("\r");
        else if (c === "b") buf.push("\b");
        else if (c === "f") buf.push("\f");
        else if (c === "u") {
          const hex = this.text.substring(this.index, this.index + 4);
          this.index = this.index + 4;
          buf.push(String.fromCharCode(parseInt(hex, 16) % 128));
        } else buf.push(c);
      } else {
        buf.push(c);
      }
      this.read();
    }
    this.read();
    return { kind: "string", value: buf.join("") };
  }
  readNumber() {
    const start = this.index;
    if (this.cur === "-") this.read();
    let c = this.cur;
    while (c !== null && ((c >= "0" && c <= "9") || c === "." || c === "e" || c === "E" || c === "+" || c === "-")) {
      this.read();
      c = this.cur;
    }
    return { kind: "number", value: Number(this.text.substring(start - 1, this.index - 1)) };
  }
}
function parse(text) {
  const p = new Parser(text);
  p.read();
  p.skipWhiteSpace();
  const v = p.readValue();
  p.skipWhiteSpace();
  if (p.cur !== null) p.error("unexpected trailing characters");
  return v;
}

let nodes, numSum, strLen;
function walk(v) {
  nodes = nodes + 1;
  const k = v.kind;
  if (k === "number") numSum = numSum + v.value;
  else if (k === "string") strLen = strLen + v.value.length;
  else if (k === "array") { for (let i = 0; i < v.items.length; i++) walk(v.items[i]); }
  else if (k === "object") {
    for (let i = 0; i < v.names.length; i++) { strLen = strLen + v.names[i].length; walk(v.values[i]); }
  }
}
function ser(v, out) {
  if (v === NULL) { out.push("null"); return; }
  const k = v.kind;
  if (k === "number") out.push(g14(v.value));
  else if (k === "string") { out.push('"'); out.push(v.value); out.push('"'); }
  else if (k === "bool") out.push(v.value ? "true" : "false");
  else if (k === "array") {
    out.push("[");
    for (let i = 0; i < v.items.length; i++) { if (i > 0) out.push(","); ser(v.items[i], out); }
    out.push("]");
  } else {
    out.push("{");
    for (let i = 0; i < v.names.length; i++) {
      if (i > 0) out.push(",");
      out.push('"'); out.push(v.names[i]); out.push('":');
      ser(v.values[i], out);
    }
    out.push("}");
  }
}

const text = buildDocument();
let rn, rs, rl, rser;
for (let it = 1; it <= N; it++) {
  const v = parse(text);
  nodes = 0; numSum = 0; strLen = 0;
  walk(v);
  const out = [];
  ser(v, out);
  rn = nodes; rs = numSum; rl = strLen; rser = out.join("").length;
}
console.log(`json\t${text.length}\t${rn}\t${rs.toFixed(6)}\t${rl}\t${rser}`);
