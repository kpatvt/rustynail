const n = Number(process.argv[2]) || 4000000;
let ok_count = 0, err_count = 0;
// Lua error() prepends "chunk:line: " (the Lua program is run as `rnlua pcall_error.lua` from bench/programs; line 3)
function risky(i) { if (i % 3 === 0) throw "pcall_error.lua:3: bad " + i; return i; }
// pcall equivalent: returns [ok, value-or-error]
function pcall(f, ...args) { try { return [true, f(...args)]; } catch (e) { return [false, e]; } }
function xpcall(f, handler, ...args) { try { return [true, f(...args)]; } catch (e) { return [false, handler(e)]; } }
for (let i = 1; i <= n; i++) {
  const [ok, v] = pcall(risky, i);
  if (ok) ok_count = ok_count + v; else err_count = err_count + v.length;
}
function thrower() { throw { code: 1 }; }
let c = 0;
for (let i = 1; i <= Math.floor(n / 4); i++) { const [ok, e] = pcall(thrower); c = c + e.code; }
let x = 0;
for (let i = 1; i <= Math.floor(n / 4); i++) { const [ok, v] = xpcall(risky, function (m) { return m.length; }, i); x = x + (ok ? v : 0); }
console.log(`${ok_count}\t${err_count}\t${c}\t${x}`);
