-- H7: coroutines yielding inside recursion
local function walk(t) if t then walk(t.l) coroutine.yield(t.v) walk(t.r) end end
local function build(lo, hi) if lo > hi then return nil end local m = (lo + hi) // 2 return { v = m, l = build(lo, m - 1), r = build(m + 1, hi) } end
local tree = build(1, 200)
for round = 1, 3 do
  local co = coroutine.wrap(function() walk(tree) end)
  local s, c = 0, 0
  for v in co do s = s + v c = c + 1 end
  print(round, s, c)
end
local function rec(n) if n == 0 then return coroutine.yield("bottom") end return rec(n - 1) + 1 end
for i = 1, 50 do
  local co = coroutine.create(rec)
  local ok, y = coroutine.resume(co, i * 4)
  local ok2, r = coroutine.resume(co, 100)
  io.write(y, ":", r, " ")
end
print()
local function fibc(n) if n < 2 then if n == 1 then coroutine.yield(1) end return n end return fibc(n - 1) + fibc(n - 2) end
local co = coroutine.wrap(function() return fibc(12) end)
local ys = 0 local r
repeat r = co() ys = ys + 1 until r ~= 1
print(ys, r)
local cos = {}
for i = 1, 5 do cos[i] = coroutine.wrap(function() return rec(30 + i) end) end
for i = 1, 5 do cos[i]() end
for i = 1, 5 do io.write(cos[i](i), " ") end print()
