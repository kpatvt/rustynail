-- H7: debug hooks (count, call / return, line) while a recursion runs; the numbers are those of the interpreter
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local c = 0
debug.sethook(function() c = c + 1 end, "", 1000)
print(fib(22), c)
debug.sethook()
local calls, rets = 0, 0
debug.sethook(function(ev) if ev == "call" then calls = calls + 1 else rets = rets + 1 end end, "cr")
print(fib(15), calls, rets)
debug.sethook()
local lines = 0
debug.sethook(function() lines = lines + 1 end, "l")
print(fib(12), lines)
debug.sethook()
local maxd = 0
local function dd(n) if n == 0 then local d = 0 while debug.getinfo(d + 1, "l") do d = d + 1 end if d > maxd then maxd = d end return 0 end return dd(n - 1) + 1 end
for i = 1, 40 do dd(i) end
print(maxd >= 40)
-- a hook installed from inside the recursion
local seen = 0
local function inner(n)
  if n == 5 then debug.sethook(function() seen = seen + 1 end, "c") end
  if n < 2 then return n end
  return inner(n - 1) + inner(n - 2)
end
print(inner(14), seen)
debug.sethook()
