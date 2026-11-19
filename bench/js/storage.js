const N = Number(process.argv[2]) || 45;
class Random {
  constructor() { this.seed = 74755; }
  next() { this.seed = (this.seed * 1309 + 13849) % 65536; return this.seed; }
}
let count;
function buildTreeDepth(depth, random) {
  count = count + 1;
  if (depth === 1) {
    const size = random.next() % 10 + 1;
    const a = [];
    for (let i = 0; i < size; i++) a.push(false);
    return a;
  } else {
    const arr = [];
    for (let i = 0; i < 4; i++) arr.push(buildTreeDepth(depth - 1, random));
    return arr;
  }
}
let total = 0, ok = 0;
for (let it = 1; it <= N; it++) {
  const random = new Random();
  count = 0;
  buildTreeDepth(7, random);
  total = total + count;
  if (count === 5461) ok = ok + 1;
}
console.log(`storage\t${ok}\t${total}`);
