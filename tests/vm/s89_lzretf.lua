-- Section 89.1 (`lzretf`): side traces of lazy-aware recursion families pop their root frame into the caller (a frame of the same closure in
-- the lazy region) and go on recording in it; the region root exits at the `LROOT` test. Everything that can observe the popped frames:
-- (1) towers of Hanoi (method recursion without loops, two call sites: the pop lands after either call), sizes in mixed order;
-- (2) fib through an upvalue and through a global (`RETURN1`: the result lands in the caller's register), sums checked;
-- (3) an error raised in the caller after the pop, caught at the top and at an inner level, with the traceback;
-- (4) debug.getinfo / debug.traceback in the caller's code after the pop (the lazy frames below are completed);
-- (5) a coroutine that yields at a leaf and in the continuation after the pop, resumed from outside;
-- (6) a count hook installed from inside the recursion after the traces exist;
-- (7) stack overflow of the recursion (message), then the recursion again;
-- (8) allocation and full collections at the leaves, finalizers that run between levels;
-- (9) the callee replaced mid-run (the upvalue and the method), and a metatable changed during the recursion;
-- (10) closures of one prototype calling each other (the caller is another closure: no pop), and a C function (pcall) between levels.

-- (1) towers
local Disk = {}
Disk.__index = Disk
local Towers = {}
Towers.__index = Towers
function Towers:push(disk, pile)
  local top = self.piles[pile]
  if top ~= nil and disk.size >= top.size then error("big on small") end
  disk.nxt = top
  self.piles[pile] = disk
end
function Towers:pop(pile)
  local top = self.piles[pile]
  if top == nil then error("empty pile") end
  self.piles[pile] = top.nxt
  top.nxt = nil
  return top
end
function Towers:moveTop(from, to)
  self:push(self:pop(from), to)
  self.moves = self.moves + 1
end
function Towers:move(n, from, to)
  if n == 1 then
    self:moveTop(from, to)
  else
    local other = 3 - from - to
    self:move(n - 1, from, other)
    self:moveTop(from, to)
    self:move(n - 1, other, to)
  end
end
local function hanoi(n)
  local t = setmetatable({ piles = {}, moves = 0 }, Towers)
  for i = n, 1, -1 do t:push(setmetatable({ size = i }, Disk), 1) end
  t:move(n, 1, 2)
  local c = 0
  local d = t.piles[2]
  while d do c = c + 1; d = d.nxt end
  return t.moves, c
end
for _, n in ipairs({ 10, 3, 9, 1, 2, 10, 5, 8, 10, 4 }) do
  print("hanoi", n, hanoi(n))
end

-- (2) fib through an upvalue and through a global
local fib
fib = function(n)
  if n < 2 then return n end
  return fib(n - 1) + fib(n - 2)
end
function gfib(n)
  if n < 2 then return n end
  return gfib(n - 1) + gfib(n - 2)
end
local s = 0
for i = 1, 22 do s = s + fib(i % 17 + 5) end
print("fib", s, fib(20), gfib(20))
local function sumto(n)
  if n == 0 then return 0, 0 end
  local a, b = sumto(n - 1)
  return a + n, b + 1
end
print("sumto", sumto(150))

-- (3) an error raised in the caller after the pop
local E = setmetatable({}, { __index = Towers })
E.__index = E
function E:moveTop(from, to)
  self.moves = self.moves + 1
  if self.moves == self.limit then error("limit " .. self.moves) end
end
for _, lim in ipairs({ 5, 300, 77, 1000, 511 }) do
  local t = setmetatable({ piles = {}, moves = 0, limit = lim }, E)
  local ok, err = pcall(t.move, t, 10, 1, 2)
  print("err", lim, ok, err, t.moves)
end
local function inner(lim)
  local t = setmetatable({ piles = {}, moves = 0, limit = lim }, E)
  local depth = 0
  local function rec(n)
    depth = depth + 1
    if n == 0 then
      local ok, err = pcall(t.move, t, 8, 1, 2)
      return ok, err
    end
    return rec(n - 1)
  end
  return rec(3)
end
print("inner", inner(200))
local ok, tb = xpcall(function()
  local t = setmetatable({ piles = {}, moves = 0, limit = 123 }, E)
  t:move(9, 1, 2)
end, debug.traceback)
print("tb", ok, (tb:gsub("0x%x+", "ADDR"):match("^[^\n]*")))
local _, nlines = tb:gsub("\n", "")
print("tb lines", nlines > 5)

-- (4) debug.getinfo / traceback in the caller after the pop
local G = setmetatable({}, { __index = Towers })
G.__index = G
local seen = {}
function G:moveTop(from, to)
  self.moves = self.moves + 1
  if self.moves % 97 == 0 then
    local d = 1
    while debug.getinfo(d, "l") do d = d + 1 end
    local info = debug.getinfo(2, "nSl")
    seen[#seen + 1] = (d - G.base) .. ":" .. tostring(info.currentline) .. ":" .. tostring(info.name)
  end
end
local t = setmetatable({ piles = {}, moves = 0 }, G)
do
  local d = 1
  while debug.getinfo(d, "l") do d = d + 1 end
  G.base = d
end
t:move(10, 1, 2)
print("getinfo", t.moves, #seen, seen[1], seen[#seen])

-- (5) yields at a leaf and in the continuation
local Y = setmetatable({}, { __index = Towers })
Y.__index = Y
function Y:moveTop(from, to)
  self.moves = self.moves + 1
  if self.moves % 50 == 0 then coroutine.yield(self.moves) end
end
local co = coroutine.wrap(function()
  local t = setmetatable({ piles = {}, moves = 0 }, Y)
  t:move(10, 1, 2)
  return "done", t.moves
end)
local ys = 0
while true do
  local a, b = co()
  if a == "done" then print("yields", ys, b) break end
  ys = ys + a
end
local cofib
cofib = function(n)
  if n < 2 then
    if n == 1 then coroutine.yield(n) end
    return n
  end
  return cofib(n - 1) + cofib(n - 2)
end
local cnt, last = 0, nil
local c2 = coroutine.wrap(function() return "r", cofib(16) end)
while true do
  local a, b = c2()
  if a == "r" then last = b break end
  cnt = cnt + 1
end
print("cofib", cnt, last)

-- (6) a count hook installed from inside
local H = setmetatable({}, { __index = Towers })
H.__index = H
local hooks = 0
function H:moveTop(from, to)
  self.moves = self.moves + 1
  if self.moves == 400 then debug.sethook(function() hooks = hooks + 1 end, "", 100) end
end
local th = setmetatable({ piles = {}, moves = 0 }, H)
th:move(10, 1, 2)
debug.sethook()
print("hook", th.moves, hooks > 0)

-- (7) stack overflow
local deep
deep = function(n)
  if n == 0 then return 0 end
  local a = deep(n - 1)
  return a + 1
end
print("deep", deep(1000))
local ok2, e2 = pcall(deep, 1e7)
print("overflow", ok2, (tostring(e2):gsub("^.-:%d+: ", "")))
print("deep again", deep(500), fib(18))

-- (8) allocation, collections and finalizers at the leaves
local A = setmetatable({}, { __index = Towers })
A.__index = A
local fins = 0
function A:moveTop(from, to)
  self.moves = self.moves + 1
  self.junk = setmetatable({ string.rep("x", self.moves % 7) }, { __gc = function() fins = fins + 1 end })
  if self.moves % 200 == 0 then collectgarbage() end
end
local ta = setmetatable({ piles = {}, moves = 0 }, A)
ta:move(10, 1, 2)
ta.junk = nil
collectgarbage()
collectgarbage()
print("gc", ta.moves, fins >= 1000)

-- (9) callee replaced mid-run; metatable changed during the recursion
local cnt9 = 0
local f9
local function f9a(n)
  cnt9 = cnt9 + 1
  if n < 2 then return n end
  if cnt9 == 3000 then
    f9 = function(m) cnt9 = cnt9 + 1; if m < 2 then return m * 10 end return f9(m - 1) + f9(m - 2) end
  end
  return f9(n - 1) + f9(n - 2)
end
f9 = f9a
print("replace", f9(18), cnt9)
local M = setmetatable({}, { __index = Towers })
M.__index = M
function M:moveTop(from, to)
  self.moves = self.moves + 1
  if self.moves == 600 then
    function M:move(n, from2, to2)
      self.moves = self.moves + 1000
    end
  end
end
local tm = setmetatable({ piles = {}, moves = 0 }, M)
tm:move(10, 1, 2)
print("method replaced", tm.moves)

-- (10) closures of one prototype calling each other; pcall between levels
local function mk(tag)
  local other
  local function f(n)
    if n == 0 then return tag end
    if n % 2 == 0 then return other(n - 1) end
    return f(n - 1)
  end
  return f, function(o) other = o end
end
local fa, seta = mk("a")
local fb, setb = mk("b")
seta(fb)
setb(fa)
local r10 = {}
for i = 1, 60 do r10[#r10 + 1] = fa(i % 9) end
print("closures", table.concat(r10, "", 1, 20))
local pc
pc = function(n)
  if n == 0 then return 1 end
  if n % 5 == 0 then
    local ok3, v = pcall(pc, n - 1)
    return v + 1
  end
  return pc(n - 1) + 1
end
local acc = 0
for i = 1, 40 do acc = acc + pc(i) end
print("pcall", acc)

-- (11) one call site that calls the closure directly or through a callable table (`__call` shifts the arguments)
local cm = {}
local callee
local tbl = setmetatable({}, cm)
local function rc(a, n)
  if n == 0 then return 1 end
  local f = (n % 3 == 0) and tbl or callee
  if f == tbl then return f(n - 1) + 2 end
  return f(a, n - 1) + 1
end
callee = rc
cm.__call = function(self, n) return rc(self, n) end
local s11 = 0
for i = 1, 300 do s11 = s11 + rc(0, i % 14) end
print("mixed __call", s11)
