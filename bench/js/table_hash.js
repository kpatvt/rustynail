const n = Number(process.argv[2]) || 200000;
// dynamic string-keyed table -> Map (the idiomatic JS choice for a large dynamic key set)
const t = new Map();
for (let i = 1; i <= n; i++) t.set("key" + i, i);
let s = 0;
for (let r = 1; r <= 5; r++) for (let i = 1; i <= n; i++) s = s + t.get("key" + i);
let c = 0;
for (const v of t.values()) c = c + v;
for (let i = 1; i <= n; i += 2) t.delete("key" + i);
let d = 0;
for (const k of t.keys()) d = d + 1;
// sparse integer keys
const h = new Map();
for (let i = 1; i <= n * 4; i++) h.set(i * 7919 % 1000003, i);
let e = 0;
for (let i = 1; i <= n * 4; i++) e = e + (h.get(i * 7919 % 1000003) || 0);
console.log(`${s}\t${c}\t${d}\t${e}`);
