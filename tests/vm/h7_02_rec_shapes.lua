-- H7: recursion shapes: fib through an upvalue, ackermann, mutual recursion, tak, a global recursive function
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
for _, n in ipairs { 5, 12, 20, 24 } do print("fib", n, fib(n)) end
local function ack(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack(m - 1, 1) end
  return ack(m - 1, ack(m, n - 1))
end
print("ack", ack(2, 100), ack(3, 4))
local even, odd
function even(n) if n == 0 then return true end return odd(n - 1) end
function odd(n) if n == 0 then return false end return even(n - 1) end
print("eo", even(3000), odd(2999), even(7))
local function tak(x, y, z) if y >= x then return z end return tak(tak(x - 1, y, z), tak(y - 1, z, x), tak(z - 1, x, y)) end
print("tak", tak(18, 12, 6))
function gfib(n) if n < 2 then return n end return gfib(n - 1) + gfib(n - 2) end
print("gfib", gfib(18))
-- the callee is replaced while the recursion runs
local g
g = function(n) if n < 2 then return n end return g(n - 1) + g(n - 2) end
local calls = 0
local g0 = g
g = function(n) calls = calls + 1 if calls == 300 then g = g0 end return g0(n) end
print("swap", g(16), calls)
