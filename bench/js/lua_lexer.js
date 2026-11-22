// Port of bench/corpus/lua_lexer.lua: a Lua-in-JS lexer (byte-level dispatch with charCodeAt/indexOf, long
// strings/comments, numbers, escapes), a token-stream "minifier" and statistics over generated source.
// `pos` is 0-based here (Lua: 1-based); Lua's two return values (kind, value) are kind returned and value in `this.val`.
// The Lua `string.find(src, "^pattern", pos)` calls are sticky regexes; `%q` is implemented by `quoteLua`.
const N = Number(process.argv[2]) || 50;

const keywords = new Set();
for (const w of "and break do else elseif end false for function goto if in local nil not or repeat return then true until while".match(/[A-Za-z]+/g)) {
  keywords.add(w);
}

const BYTE_0 = 48, BYTE_9 = 57;
const BYTE_a = 97, BYTE_z = 122, BYTE_A = 65, BYTE_Z = 90, BYTE_UNDER = 95;
const BYTE_NL = 10, BYTE_CR = 13, BYTE_SPACE = 32, BYTE_TAB = 9;
const BYTE_DASH = 45, BYTE_LBRACKET = 91, BYTE_EQ = 61, BYTE_QUOTE = 39, BYTE_DQUOTE = 34, BYTE_DOT = 46, BYTE_BSLASH = 92;

function isAlpha(b) { return (b >= BYTE_a && b <= BYTE_z) || (b >= BYTE_A && b <= BYTE_Z) || b === BYTE_UNDER; }
function isDigit(b) { return b >= BYTE_0 && b <= BYTE_9; }
function isAlnum(b) { return isAlpha(b) || isDigit(b); }

const RE_HEXNUM = /0[xX][0-9a-fA-F.]+[pP]?[+-]?[0-9a-fA-F]*/y;
const RE_EXPNUM = /[0-9]*\.?[0-9]*[eE][+-]?[0-9]+/y;
const RE_NUM = /[0-9]*\.?[0-9]*/y;
const RE_DEC = /[0-9][0-9]?[0-9]?/y;

const escapes = new Map([["n", "\n"], ["t", "\t"], ["r", "\r"], ["a", "\x07"], ["b", "\b"], ["f", "\f"], ["v", "\v"],
  ["\\", "\\"], ['"', '"'], ["'", "'"], ["\n", "\n"]]);
const ops3 = new Set(["..."]);
const ops2 = new Set(["==", "~=", "<=", ">=", "..", "::", "//", "<<", ">>"]);

class Lexer {
  constructor(src) { this.src = src; this.pos = 0; this.line = 1; this.val = ""; this.start = 0; }

  // at '[': returns the level (and sets this.start after the second '[') or -1
  skipLongBracket() {
    let p = this.pos + 1;
    let level = 0;
    while (this.src.charCodeAt(p) === BYTE_EQ) { level = level + 1; p = p + 1; }
    if (this.src.charCodeAt(p) === BYTE_LBRACKET) { this.start = p + 1; return level; }
    return -1;
  }

  readLong(level, start) {
    const close = "]" + "=".repeat(level) + "]";
    const s = this.src.indexOf(close, start);
    if (s < 0) throw new Error("unfinished long string at line " + this.line);
    const body = this.src.substring(start, s);
    for (let i = body.indexOf("\n"); i >= 0; i = body.indexOf("\n", i + 1)) this.line = this.line + 1;
    this.pos = s + close.length;
    return body;
  }

  readString(q) {
    const buf = [];
    let p = this.pos + 1;
    const src = this.src;
    while (true) {
      const b = src.charCodeAt(p);
      if (b !== b) throw new Error("unfinished string");
      if (b === q) break;
      if (b === BYTE_BSLASH) {
        const c = src.charAt(p + 1);
        const esc = escapes.get(c);
        if (esc !== undefined) {
          buf.push(esc); p = p + 2;
        } else if (c >= "0" && c <= "9" && c.length === 1) {
          RE_DEC.lastIndex = p + 1;
          const d = RE_DEC.exec(src)[0];
          buf.push(String.fromCharCode(Number(d))); p = p + 1 + d.length;
        } else if (c === "x") {
          buf.push(String.fromCharCode(parseInt(src.substring(p + 2, p + 4), 16))); p = p + 4;
        } else {
          throw new Error("bad escape \\" + c);
        }
      } else {
        buf.push(String.fromCharCode(b)); p = p + 1;
      }
    }
    this.pos = p + 1;
    return buf.join("");
  }

  next() {
    const src = this.src;
    while (true) {
      const b = src.charCodeAt(this.pos);
      if (b !== b) return "<eof>";
      if (b === BYTE_NL) {
        this.line = this.line + 1; this.pos = this.pos + 1;
      } else if (b === BYTE_SPACE || b === BYTE_TAB || b === BYTE_CR) {
        this.pos = this.pos + 1;
      } else if (b === BYTE_DASH && src.charCodeAt(this.pos + 1) === BYTE_DASH) {
        this.pos = this.pos + 2;
        let level = -1;
        if (src.charCodeAt(this.pos) === BYTE_LBRACKET) level = this.skipLongBracket();
        if (level >= 0) {
          this.readLong(level, this.start);
        } else {
          let e = src.indexOf("\n", this.pos);
          if (e < 0) e = src.length;
          this.pos = e;
        }
      } else {
        break;
      }
    }
    const b = src.charCodeAt(this.pos);
    const start = this.pos;
    if (isAlpha(b)) {
      let p = this.pos + 1;
      while (true) {
        const c = src.charCodeAt(p);
        if (c !== c || !isAlnum(c)) break;
        p = p + 1;
      }
      const word = src.substring(start, p);
      this.pos = p;
      this.val = word;
      if (keywords.has(word)) return "keyword";
      return "name";
    } else if (isDigit(b) || (b === BYTE_DOT && isDigit(src.charCodeAt(this.pos + 1) || 0))) {
      let re = RE_HEXNUM;
      re.lastIndex = start;
      let m = re.exec(src);
      if (m === null) { re = RE_EXPNUM; re.lastIndex = start; m = re.exec(src); }
      if (m === null) { re = RE_NUM; re.lastIndex = start; m = re.exec(src); }
      this.pos = start + m[0].length;
      this.val = m[0];
      return "number";
    } else if (b === BYTE_QUOTE || b === BYTE_DQUOTE) {
      this.val = this.readString(b);
      return "string";
    } else if (b === BYTE_LBRACKET) {
      const level = this.skipLongBracket();
      if (level >= 0) { this.val = this.readLong(level, this.start); return "string"; }
      this.pos = this.pos + 1;
      this.val = "[";
      return "op";
    } else {
      const t3 = src.substring(start, start + 3);
      if (ops3.has(t3)) { this.pos = start + 3; this.val = t3; return "op"; }
      const t2 = src.substring(start, start + 2);
      if (ops2.has(t2)) { this.pos = start + 2; this.val = t2; return "op"; }
      this.pos = start + 1;
      this.val = String.fromCharCode(b);
      return "op";
    }
  }
}

// string.format("%q", s) for a string value
function quoteLua(s) {
  const buf = ['"'];
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === 34 || c === 92 || c === 10) {
      buf.push("\\", s.charAt(i));
    } else if (c < 32 || c === 127) {
      const n = s.charCodeAt(i + 1);
      if (n >= 48 && n <= 57) buf.push("\\" + String(c).padStart(3, "0"));
      else buf.push("\\" + c);
    } else {
      buf.push(s.charAt(i));
    }
  }
  buf.push('"');
  return buf.join("");
}

// generate a deterministic synthetic Lua source file
function genSource(nfuncs) {
  const parts = [];
  for (let i = 1; i <= nfuncs; i++) {
    parts.push(String.raw`-- function number ${i}: computes things
local function f_${i}(a, b, ...)
  local t = { x = ${i}, y = 0x${(i * 31).toString(16).toUpperCase()}, s = "str\t${i}\n", ["key ${i}"] = 'single\'s', 3.25e-2, 1e10 }
  --[==[ long comment
  with ]] inside ]==]
  if a >= b and not (a ~= ${i}) or #t > 2 then
    for k = 1, ${i % 9} do t[k] = (t[k] or 0) + k * a // 2 .. "" end
  elseif a <= b then
    return [[long
string ${i}]], ...
  end
  while b < a do b = b + 1; a = a - 1 end
  return t.x + t.y, select('#', ...)
end
`);
  }
  return parts.join("");
}

const src = genSource(300);
const counts = {};
let totalTokens = 0, lines = 0;
let minParts = "";
for (let r = 1; r <= N; r++) {
  const lx = new Lexer(src);
  const out = [];
  let prev = "op";
  while (true) {
    const kind = lx.next();
    if (kind === "<eof>") break;
    let val = lx.val;
    totalTokens = totalTokens + 1;
    counts[kind] = (counts[kind] || 0) + 1;
    // minifier: re-emit with minimal spacing
    if (kind === "string") {
      val = quoteLua(val);
    }
    if ((kind === "name" || kind === "keyword" || kind === "number") && (prev === "name" || prev === "keyword" || prev === "number")) {
      out.push(" ");
    }
    out.push(val);
    prev = kind;
  }
  lines = lines + lx.line;
  minParts = out.join("");
}
const kinds = Object.keys(counts);
kinds.sort();
const s = [];
for (const k of kinds) s.push(k + "=" + counts[k]);
console.log(`tokens\t${totalTokens}\tlines\t${lines}\t${s.join(" ")}`);
console.log(`minified\t${src.length}\t->\t${minParts.length}`);
console.log(minParts.substring(0, 200));
