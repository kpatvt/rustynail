const N = Number(process.argv[2]) || 350;
class Element {
  constructor(v) { this.val = v; this.nxt = null; }
  length() {
    if (this.nxt === null) return 1;
    return 1 + this.nxt.length();
  }
}
function makeList(length) {
  if (length === 0) return null;
  const e = new Element(length);
  e.nxt = makeList(length - 1);
  return e;
}
function isShorterThan(x, y) {
  let xTail = x, yTail = y;
  while (yTail !== null) {
    if (xTail === null) return true;
    xTail = xTail.nxt;
    yTail = yTail.nxt;
  }
  return false;
}
function tail(x, y, z) {
  if (isShorterThan(y, x)) {
    return tail(tail(x.nxt, y, z), tail(y.nxt, z, x), tail(z.nxt, x, y));
  } else {
    return z;
  }
}
let sum = 0;
for (let i = 1; i <= N; i++) {
  const r = tail(makeList(15), makeList(10), makeList(6));
  sum = sum + r.length();
}
console.log(`list\t${sum}`);
