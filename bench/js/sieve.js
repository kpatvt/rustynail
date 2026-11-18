const N = Number(process.argv[2]) || 12000000;
let total = 0;
for (let r = 1; r <= 3; r++) {
  const flags = [];
  for (let i = 2; i <= N; i++) flags[i] = true;
  for (let i = 2; i <= Math.floor(Math.sqrt(N)); i++) {
    if (flags[i]) for (let j = i * i; j <= N; j += i) flags[j] = false;
  }
  let c = 0;
  for (let i = 2; i <= N; i++) if (flags[i]) c = c + 1;
  total = total + c;
}
console.log(total);
