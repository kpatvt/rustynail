const n = Number(process.argv[2]) || 300000;
const t = [];
for (let i = 1; i <= n; i++) t.push((i * 7919) % 100003);
let s = 0;
for (let i = 0; i < t.length; i++) s = s + t[i];
for (let r = 1; r <= 5; r++) for (const v of t) s = s + v;
const u = [];
for (let i = 1; i <= Math.floor(n / 4); i++) u.push(i);
for (let i = 1; i <= Math.floor(n / 8); i++) s = s + u.pop();
for (let i = 1; i <= 2000; i++) { u.unshift(i); s = s + u.shift(); }
// table.sort with default < on numbers: compare numerically (Array.sort needs a comparator)
t.sort((a, b) => a - b);
const m = [];
for (let i = 1; i <= Math.floor(n / 5); i++) m.push((i * 31337) % 65537);
m.sort((a, b) => b - a);
console.log(`${s}\t${t[0]}\t${t[t.length - 1]}\t${m[0]}\t${m[m.length - 1]}\t${u.length}\t200`);
