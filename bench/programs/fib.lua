local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local n = tonumber(arg and arg[1]) or 37
print(fib(n))
