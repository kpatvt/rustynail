const n = Number(process.argv[2]) || 5000000;
let last = 42;
function rnd() { last = (last * 3877 + 29573) % 139968; return last; }
const t = [];
const bases = ["a", "c", "g", "t"];
for (let i = 1; i <= n; i++) t.push(bases[rnd() % 4]);
const seq = t.join("");
const result = [];
for (let frame = 1; frame <= 4; frame++) {
  const counts = new Map();
  for (let i = 0; i <= seq.length - frame; i++) { const k = seq.substring(i, i + frame); counts.set(k, (counts.get(k) || 0) + 1); }
  const keys = [];
  for (const k of counts.keys()) keys.push(k);
  keys.sort((a, b) => counts.get(a) > counts.get(b) ? -1 : counts.get(a) < counts.get(b) ? 1 : (a < b ? -1 : a > b ? 1 : 0));
  result.push(keys[0] + "=" + counts.get(keys[0]));
}
console.log(result.join(" "));
