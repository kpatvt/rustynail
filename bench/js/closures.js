const n = Number(process.argv[2]) || 30000000;
function counter() { let c = 0; return function (d) { c = c + d; return c; }; }
let total = 0;
for (let i = 1; i <= Math.floor(n / 10); i++) {
  const f = counter();
  for (let j = 1; j <= 10; j++) total = total + f(j);
}
function compose(f, g) { return function (x) { return f(g(x)); }; }
const inc = function (x) { return x + 1; };
let h = inc;
for (let i = 1; i <= 20; i++) h = compose(h, inc);
for (let i = 1; i <= Math.floor(n / 20); i++) total = total + h(i);
console.log(String(total));
