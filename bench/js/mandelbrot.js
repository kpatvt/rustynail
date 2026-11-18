const N = Number(process.argv[2]) || 1500;
let count = 0;
for (let y = 0; y < N; y++) {
  const ci = 2.0 * y / N - 1.0;
  for (let x = 0; x < N; x++) {
    const cr = 2.0 * x / N - 1.5;
    let zr = 0.0, zi = 0.0, i = 0;
    while (i < 50 && zr * zr + zi * zi < 4.0) { const t = zr * zr - zi * zi + cr; zi = 2.0 * zr * zi + ci; zr = t; i = i + 1; }
    if (i === 50) count = count + 1;
  }
}
console.log(count);
