-- K2: recursion found through a table field / vararg / coroutine / mutual recursion / deep recursion: the same-prototype-on-the-stack rule must only change
-- policy, never results
local M = {}
function M.fact(n) if n <= 1 then return 1 end return n * M.fact(n - 1) end
local s = 0
for i = 1, 300 do s = s + M.fact(i % 12) end
print("field", s)
local function va(...) local n = select("#", ...) if n == 0 then return 0 end return n + va(select(2, ...)) end
local s2 = 0
for i = 1, 200 do s2 = s2 + va(1, 2, 3, i % 4) end
print("vararg", s2)
local ev, od
function ev(n) if n == 0 then return true end return od(n - 1) end
function od(n) if n == 0 then return false end return ev(n - 1) end
local c = 0
for i = 1, 400 do if ev(i % 9) then c = c + 1 end end
print("mutual", c)
local O = {}
O.__index = O
function O:depth(n) if n == 0 then return 0 end return 1 + self:depth(n - 1) end
local o = setmetatable({}, O)
print("deep", o:depth(150), pcall(o.depth, o, 190))
local co = coroutine.wrap(function()
  for i = 1, 200 do coroutine.yield(o:depth(i % 7)) end
end)
local cs = 0
for i = 1, 200 do cs = cs + co() end
print("co", cs)
-- not recursive: a function called from a Lua caller of another prototype, many leaf samples
local function leaf(x) if x > 0 then return x end return math.floor(-x) end
local function caller(n) local t = 0 for i = 1, n do t = t + leaf(i - 50) end return t end
local ct = 0
for i = 1, 60 do ct = ct + caller(100) end
print("leaf", ct)
