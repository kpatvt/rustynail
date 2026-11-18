const n = Number(process.argv[2]) || 400;
function mk(n, f) { const m = []; for (let i = 1; i <= n; i++) { const r = []; for (let j = 1; j <= n; j++) r[j] = f(i, j); m[i] = r; } return m; }
const a = mk(n, (i, j) => (i + j) / n), b = mk(n, (i, j) => (i * j % 7) / n);
const c = [];
for (let i = 1; i <= n; i++) {
  const ai = a[i], ci = [];
  for (let j = 1; j <= n; j++) {
    let s = 0.0;
    for (let k = 1; k <= n; k++) s = s + ai[k] * b[k][j];
    ci[j] = s;
  }
  c[i] = ci;
}
let tr = 0;
for (let i = 1; i <= n; i++) tr = tr + c[i][i];
console.log(tr.toFixed(6));
