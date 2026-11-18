function A(i, j) { const ij = i + j - 1; return 1.0 / (ij * (ij - 1) * 0.5 + i); }
function Av(x, y, N) { for (let i = 1; i <= N; i++) { let a = 0; for (let j = 1; j <= N; j++) a = a + x[j] * A(i, j); y[i] = a; } }
function Atv(x, y, N) { for (let i = 1; i <= N; i++) { let a = 0; for (let j = 1; j <= N; j++) a = a + x[j] * A(j, i); y[i] = a; } }
function AtAv(x, y, t, N) { Av(x, t, N); Atv(t, y, N); }
const N = Number(process.argv[2]) || 1000;
const u = [], v = [], t = [];
for (let i = 1; i <= N; i++) u[i] = 1;
for (let i = 1; i <= 10; i++) { AtAv(u, v, t, N); AtAv(v, u, t, N); }
let vBv = 0, vv = 0;
for (let i = 1; i <= N; i++) { const ui = u[i], vi = v[i]; vBv = vBv + ui * vi; vv = vv + vi * vi; }
console.log(Math.sqrt(vBv / vv).toFixed(9));
