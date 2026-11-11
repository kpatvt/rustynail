-- BUNDLE-K L1 (lazy CallInfo): recursion shapes around the region boundary: roots entered from C (pcall, sort, gsub), stack overflow at depth,
-- errors at depth with tracebacks, coroutines created inside the recursion, mutual recursion, varargs, tail calls, results used by C.
local function norm(s) return (tostring(s):gsub("0x%x+", "PTR")) end

local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
for i = 1, 40 do fib(12) end

-- roots entered from C: pcall, table.sort comparator, gsub callback, coroutine body
print(pcall(fib, 15))
print(select(2, pcall(fib, "x")))
local t = {}
for i = 1, 40 do t[i] = (i * 17) % 23 end
table.sort(t, function(a, b) return fib(a % 9) < fib(b % 9) or (fib(a % 9) == fib(b % 9) and a < b) end)
print(table.concat(t, ","))
print((("abcdefgh"):gsub(".", function(c) return fib(#c + 9) end)))
print(coroutine.wrap(function() return fib(14) end)())

-- stack overflow after warm-up (single self-call chain: only pushes, then returns), message and recovery
local function deep(n) return 1 + deep(n + 1) end
for i = 1, 30 do pcall(function() local function s(n) if n == 0 then return 0 end return 1 + s(n - 1) end return s(50) end) end
local ok, m = pcall(deep, 1) print(ok, (norm(m):gsub("^.-stack overflow", "stack overflow")))
print(fib(16))
-- again, inside a coroutine (a smaller C stack budget), and inside pcall inside pcall
print(coroutine.wrap(function() local ok, m = pcall(deep, 1) return ok, (norm(m):gsub("^.-stack overflow", "stack overflow")) end)())
print(pcall(pcall, deep, 1))

-- error at depth with traceback (the traceback is cut in the middle by the reference's rule)
local function bad(n) if n == 0 then error("bottom") end return bad(n - 1) + 1 end
for i = 1, 30 do pcall(bad, 5) end
print(xpcall(bad, function(m) local tb = debug.traceback(m, 1) return (norm(tb):gsub("\n", "|")) end, 60))
print(xpcall(bad, function(m) return #debug.traceback(m, 1) end, 1000))

-- error with a non-string value from depth, error in the message handler
print(pcall(function() local function e(n) if n == 0 then error(setmetatable({}, { __tostring = function() return "obj" end })) end return e(n - 1) + 1 end for i = 1, 20 do pcall(e, 3) end return e(30) end))
print(xpcall(bad, function(m) error("handler fails") end, 40))

-- coroutines created inside the recursion and run to completion
local cnt = 0
local function cf(n)
  if n < 2 then
    cnt = cnt + 1
    if cnt % 13 == 0 then
      local co = coroutine.create(function(a) local b = coroutine.yield(fib(a)) return b + fib(a + 1) end)
      local _, x = coroutine.resume(co, 6) local _, y = coroutine.resume(co, 100)
      return n + (x + y) % 3
    end
    return n
  end
  return cf(n - 1) + cf(n - 2)
end
for i = 1, 5 do print(cf(14)) end

-- mutual recursion, vararg recursion, multiple results, tail calls
local ev, od
function ev(n) if n == 0 then return true end return od(n - 1) end
function od(n) if n == 0 then return false end return ev(n - 1) end
local function va(n, ...) if n == 0 then return select("#", ...) end return va(n - 1, n, ...) end
local function two(n) if n < 2 then return n, n + 1 end local a = two(n - 1) local b = two(n - 2) return a + b, a end
local function tail(n, acc) if n == 0 then return acc end return tail(n - 1, acc + n) end
for i = 1, 30 do ev(20) va(5) two(8) tail(40, 0) end
print(ev(101), od(101), va(40), two(15), tail(10000, 0))

-- ackermann: deep pushes after returns
local function ack(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack(m - 1, 1) end
  return ack(m - 1, ack(m, n - 1))
end
for i = 1, 20 do ack(2, 3) end
print(ack(2, 5), ack(3, 4))

-- floats and wrong types at depth (guards fail at depth: exits with a long chain)
local function fl(n) if n < 2 then return n end return fl(n - 1) + fl(n - 2) end
for i = 1, 40 do fl(10) end
print(fl(12), fl(12.5), fl(2 ^ 3), pcall(fl, {}))
print(pcall(fl, "10"))
