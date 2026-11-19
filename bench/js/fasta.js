const n = Number(process.argv[2]) || 10000000;
const IM = 139968, IA = 3877, IC = 29573;
let last = 42;
function random(max) { last = (last * IA + IC) % IM; return max * last / IM; }
const alu = "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGGGAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGACCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAATACAAAAATTAGCCGGG";
let buf = [], check = 0;
const out = [];
for (let i = 1; i <= n; i++) {
  const r = random(1.0);
  const c = r < 0.3 ? "a" : r < 0.5 ? "c" : r < 0.8 ? "g" : "t";
  buf.push(c);
  if (buf.length === 60) { const line = buf.join(""); check = check + line.length + line.charCodeAt(0); out.push(line); buf = []; }
}
const rep = alu.repeat(Math.floor(n / 100)).substring(0, n);
console.log(`${check}\t${out.length}\t${rep.length}\t${rep.charCodeAt(n - 1)}`);
