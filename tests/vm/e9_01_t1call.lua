-- E9 (`t1call`, docs/JIT_TRACE.md section 37): the tier-1 call protocol. A frame the tier-1 `CALL` pushes is marked and holds its site's
-- resume cell; its tier-1 return jumps through the cell. Every case mixes frames the tier-1 `CALL` pushed with frames something else pushed
-- (the interpreter, the general call helper, a C function, a metamethod, a coroutine) on the same `CallInfo` nodes, several call sites of one
-- callee with different result counts, tail calls inside marked frames, and the observers (hooks, tracebacks, errors, stack overflow).

-- (1) one callee, call sites with 0 / 1 / 2 / all results, constants used right after each return
local function f(a, b) return a + (b or 0), a end
local function c1(n) local s = 0 for i = 1, n do local x = f(i, 2) s = s + x end return "c1:" .. s end
local function c2(n) local s = 0 for i = 1, n do local x, y = f(i, 1) s = s + x * y end return "c2:" .. s end
local function c3(n) local s = 0 for i = 1, n do f(i, 3) s = s + 1 end return "c3:" .. s end
local function c4(n) local s = 0 for i = 1, n do s = s + select("#", f(i, 4)) end return "c4:" .. s end
for r = 1, 3 do print(c1(200), c2(200), c3(200), c4(200)) end

-- (2) the same callee and depth, alternately pushed by the flat call (two arguments) and by the general helper (one argument: nil-filled),
-- so a `CallInfo` node holds a stale resume cell when the helper reuses it
local function g1(n) local s = 0 for i = 1, n do s = s + f(i, i) * 3 end return s end
local function g2(n) local s = 0 for i = 1, n do s = s + f(i) * 5 end return s end
local tot = 0
for r = 1, 200 do tot = tot + g1(3) + g2(3) end
print("mix", tot)

-- (3) tail calls in a marked frame: a Lua callee (inline tail call), a C function (the helper finishes and pops), `select`
local function t1(x) return f(x, 7) end
local function t2(x) return string.format("%d|%s", x, "k") end
local function t3(...) return select(2, ...) end
local function tc(n)
  local s, str = 0, ""
  for i = 1, n do s = s + t1(i) + t3(i, i * 2, 9) str = t2(i) end
  return s, str
end
print("tail", tc(300))

-- (4) vararg callees, results of all counts
local function va(...) return select("#", ...), ... end
local function vb(n) local s = 0 for i = 1, n do local k, a, b = va(i, i + 1) s = s + k + a + b end return s end
local function vc(n) local t = {} for i = 1, n do t[#t + 1] = select("#", va(i, nil, i)) end return #t, t[n] end
print("vararg", vb(200), vc(200))

-- (5) recursion through upvalue and method calls, with constants after the calls
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local Obj = {}
Obj.__index = Obj
function Obj.new(v) return setmetatable({ v = v }, Obj) end
function Obj:get() return self.v end
function Obj:sum(n) if n == 0 then return self:get() end return self:sum(n - 1) + self.v * 2 end
local o = Obj.new(3)
print("rec", fib(18), o:sum(30), o:get() .. "x")

-- (6) metamethod frames (pushed by helpers, not by the flat call) between marked frames
local V = {}
V.__index = function(t, k) return k .. "!" end
V.__add = function(a, b) return f(a.n, b.n) end
V.__eq = function(a, b) return a.n == b.n end
local function mm(n)
  local a, b = setmetatable({ n = 1 }, V), setmetatable({ n = 2 }, V)
  local s, str = 0, ""
  for i = 1, n do s = s + (a + b) + (a == b and 1 or 0) str = a.key end
  return s, str
end
print("mm", mm(200))

-- (7) errors from depth (the unwinding drops marked frames; the nodes are reused by other pushers), messages and tracebacks
local function e1(n, at)
  if n == at then error("boom at " .. n) end
  if n == 0 then return 0 end
  return e1(n - 1, at) + 1
end
local s7, msg = 0, nil
for i = 1, 100 do
  local ok, r = pcall(e1, 12, (i % 9 == 0) and 4 or -1)
  if ok then s7 = s7 + r else msg = r end
end
print("err", s7, msg)
local function tb(n) if n == 0 then return debug.traceback("tb", 1) end return (tb(n - 1)) end
local trace = tb(4)
-- (the number of lines depends on how the chunk was started: only the frames of `tb` are counted)
local nl = 0
for _ in trace:gmatch("in %a+ 'tb'") do nl = nl + 1 end
for _ in trace:gmatch("in upvalue 'tb'") do nl = nl + 1 end
print("traceback", trace:sub(1, 2), nl)
local function where(n) if n == 0 then local i = debug.getinfo(2, "l") return i.currentline end return (where(n - 1)) end
print("line", where(3))

-- (8) stack overflow through marked frames: same depth and message as the interpreter
local depth = 0
local function inf(n) depth = n return inf(n + 1) + 1 end
local ok, e = pcall(inf, 1)
print("overflow", ok, (string.find(tostring(e), "stack overflow", 1, true)) ~= nil, depth > 1000)
local d1 = depth
depth = 0
pcall(inf, 1)
print("same depth", depth == d1)

-- (9) a coroutine that yields from inside marked frames and is resumed from another frame
local function inner(x) local y = coroutine.yield(x * 2) return y + x end
local function middle(x) local r = inner(x) return r * 3 end
local co = coroutine.wrap(function(n) local s = 0 for i = 1, n do s = s + middle(i) end return s end)
local cs = co(5)
for i = 1, 5 do cs = co(i) end
print("coro", cs)

-- (10) a C function calling Lua (sort comparator) that makes flat calls itself
local function key(x) return (x * 7) % 11 end
local function cmp(a, b) return key(a) < key(b) or (key(a) == key(b) and a < b) end
local arr = {}
for i = 1, 60 do arr[i] = i end
table.sort(arr, cmp)
print("sort", arr[1], arr[30], arr[60])

-- (11) hooks installed from inside a marked frame (count and line hooks: the counts must be the interpreter's), then removed
local hc, lc = 0, 0
local function work(n) local s = 0 for i = 1, n do s = s + f(i, 1) end return s end
local function arm(kind)
  if kind == 1 then debug.sethook(function() hc = hc + 1 end, "", 5)
  else debug.sethook(function(ev, l) lc = lc + 1 end, "l") end
  return work(20)
end
local h1 = arm(1) + work(30)
debug.sethook()
local h2 = arm(2) + work(10)
debug.sethook()
print("hooks", h1, h2, hc, lc)

-- (12) the callee replaced while marked frames of the old one are suspended
local cb
local function step(n) if n == 0 then return cb(1) end return step(n - 1) + 1 end
cb = function(x) return x * 10 end
local before = step(5)
cb = function(x) cb = function(y) return y end return x * 100 end
print("replace", before, step(5), step(5))

-- (13) upvalues closed at the return of a marked frame (`RETURN` with `k`)
local function mk(n) local acc = n local function add(d) acc = acc + d return acc end return add(1), add end
local function useclo(n) local s = 0 for i = 1, n do local v, fn = mk(i) s = s + v + fn(2) end return s end
print("close", useclo(100))

-- (14) a recursive function whose loop runs after its call: the innermost activation runs the loop first, so tier 1.5 recompiles the
-- function while the outer activations wait at their call (they return into the new version)
local function rc(d, n)
  local s = 0
  if d > 0 then s = rc(d - 1, n) * 2 end
  for i = 1, n do s = s + i % 7 end
  return s
end
local s14 = 0
for r = 1, 40 do s14 = s14 + rc(5, 300) end
print("recompile", s14)
