const N = Number(process.argv[2]) || 100;
class P {
  constructor() { this.count = 0; this.v = [0, 0, 0, 0, 0, 0]; }
  swap(i, j) {
    const tmp = this.v[i];
    this.v[i] = this.v[j];
    this.v[j] = tmp;
  }
  permute(n) {
    this.count = this.count + 1;
    if (n !== 0) {
      const n1 = n - 1;
      this.permute(n1);
      for (let i = n1; i >= 0; i--) {
        this.swap(n1, i);
        this.permute(n1);
        this.swap(n1, i);
      }
    }
  }
}
let total = 0, ok = 0;
for (let it = 1; it <= N; it++) {
  const p = new P();
  p.permute(6);
  total = total + p.count;
  if (p.count === 8660) ok = ok + 1;
}
console.log(`permute\t${ok}\t${total}`);
