local function fact(n) if n <= 1 then return 1 end return n * fact(n - 1) end
print(fact(10), fact(20))
local fib
fib = function(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
print(fib(20))
local even, odd
function even(n) if n == 0 then return true end return odd(n - 1) end
function odd(n) if n == 0 then return false end return even(n - 1) end
print(even(10), odd(7), even(7))
local function ack(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack(m - 1, 1) end
  return ack(m - 1, ack(m, n - 1))
end
print(ack(2, 3))
