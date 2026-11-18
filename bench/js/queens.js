const N = Number(process.argv[2]) || 13;
let count = 0;
const cols = [], d1 = [], d2 = [];
function place(r) {
  if (r > N) { count = count + 1; return; }
  for (let c = 1; c <= N; c++) {
    if (!cols[c] && !d1[r + c] && !d2[r - c + N]) {
      cols[c] = d1[r + c] = d2[r - c + N] = true;
      place(r + 1);
      cols[c] = d1[r + c] = d2[r - c + N] = false;
    }
  }
}
place(1);
console.log(count);
