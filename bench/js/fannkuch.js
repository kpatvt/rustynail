function fannkuch(n) {
  const p = [], q = [], s = [];
  let sign = 1, maxflips = 0, sum = 0;
  for (let i = 1; i <= n; i++) { p[i] = i; q[i] = i; s[i] = i; }
  for (;;) {
    let q1 = p[1];
    if (q1 !== 1) {
      for (let i = 2; i <= n; i++) q[i] = p[i];
      let flips = 1;
      for (;;) {
        const qq = q[q1];
        if (qq === 1) { sum = sum + sign * flips; if (flips > maxflips) maxflips = flips; break; }
        q[q1] = q1;
        if (q1 >= 4) { let i = 2, j = q1 - 1; do { const tmp = q[i]; q[i] = q[j]; q[j] = tmp; i = i + 1; j = j - 1; } while (i < j); }
        q1 = qq; flips = flips + 1;
      }
    }
    if (sign === 1) { const tmp = p[2]; p[2] = p[1]; p[1] = tmp; sign = -1; }
    else {
      const tmp = p[2]; p[2] = p[3]; p[3] = tmp; sign = 1;
      for (let i = 3; i <= n; i++) {
        const sx = s[i];
        if (sx !== 1) { s[i] = sx - 1; break; }
        if (i === n) return [sum, maxflips];
        s[i] = i;
        const t = p[1]; for (let j = 1; j <= i; j++) p[j] = p[j + 1]; p[i + 1] = t;
      }
    }
  }
}
const n = Number(process.argv[2]) || 10;
const [sum, flips] = fannkuch(n);
console.log(sum + "\nPfannkuchen(" + n + ") = " + flips);
