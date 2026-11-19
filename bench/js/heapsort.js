const n = Number(process.argv[2]) || 1200000;
const a = [];
let seed = 42;
// Lua uses exact 64-bit ints: (seed*1103515245+12345) % 2^31. Doubles lose precision here, so use the low 32 bits via imul.
for (let i = 1; i <= n; i++) { seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff; a[i] = seed; }
function heapsort(a, n) {
  function sift(lo, hi) {
    let root = lo;
    for (;;) {
      let child = root * 2;
      if (child > hi) break;
      if (child < hi && a[child] < a[child + 1]) child = child + 1;
      if (a[root] < a[child]) { const t = a[root]; a[root] = a[child]; a[child] = t; root = child; } else break;
    }
  }
  for (let i = Math.floor(n / 2); i >= 1; i--) sift(i, n);
  for (let i = n; i >= 2; i--) { const t = a[1]; a[1] = a[i]; a[i] = t; sift(1, i - 1); }
}
heapsort(a, n);
let ok = true;
for (let i = 2; i <= n; i++) if (a[i - 1] > a[i]) ok = false;
console.log(`${ok}\t${a[1]}\t${a[n]}`);
