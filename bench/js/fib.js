function fib(n) { if (n < 2) return n; return fib(n - 1) + fib(n - 2); }
const n = Number(process.argv[2]) || 37;
console.log(fib(n));
