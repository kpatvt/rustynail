-- E6 (`dlink`, docs/JIT_TRACE.md section 36.3): direct exit-to-side links in lazy-aware recursion families. A guard whose exit has a side trace
-- attached branches straight to the side trace's link entry, which stores the exit map and keeps the budget in its register. Every case takes
-- exits with side traces many times, with values in the exit map (computed before the guard), several side traces per family, side traces that
-- are flushed and re-attached, and recursion that leaves the region in the middle.

-- (1) fib with a value computed before the leaf guard (the leaf's exit map is not empty)
local function fib(n)
  local a = n * 3
  if n < 2 then return n + a end
  return fib(n - 1) + fib(n - 2) - a
end
local s1 = 0
for i = 1, 30 do s1 = s1 + fib(12 + i % 3) end
print("fib", s1)

-- (2) a tree whose leaves take one of four paths (four side traces of the leaf exit chain), with float, string and table values live
local function tree(n, k)
  local f = n * 0.5
  local s = "t" .. (k % 3)
  if n == 0 then
    if k % 4 == 0 then return f + #s
    elseif k % 4 == 1 then return f - 1
    elseif k % 4 == 2 then return #s * 2
    else return 7 end
  end
  return tree(n - 1, k + 1) + tree(n - 1, k + 2) + f
end
local s2 = 0
for i = 1, 40 do s2 = s2 + tree(7, i) end
print("tree", s2)

-- (3) method recursion (a loop family: E4 / E5) whose leaves have side traces
local P = {}
P.__index = P
function P:perm(n)
  self.count = self.count + 1
  if n ~= 0 then
    local n1 = n - 1
    self:perm(n1)
    for i = n1, 0, -1 do self:perm(n1) end
  elseif self.count % 5 == 0 then
    self.odd = self.odd + 1
  end
end
local s3 = 0
for it = 1, 15 do local p = setmetatable({ count = 0, odd = 0 }, P) p:perm(5) s3 = s3 + p.count + p.odd end
print("perm", s3)

-- (4) a leaf side trace that inlines a function made by `load`: dropped and collected between the rounds, its prototype takes the side trace with it
-- (a flush of the side trace alone, the parent stays): the parent's exit must branch to its stub again, then to the next round's side trace
local s4 = 0
local function fibf(n, f)
  local a = n * 3
  if n < 2 then return f(n) + a end
  return fibf(n - 1, f) + fibf(n - 2, f) - a
end
for round = 1, 6 do
  local f = load("return function(x) return x * " .. round .. " + 1 end")()
  for i = 1, 20 do s4 = s4 + fibf(12, f) end
  f = nil
  collectgarbage()
  collectgarbage()
end
print("reload", s4)

-- (5) the region left in the middle: a leaf that calls a C function on some calls (the exits around it with and without side traces)
local function walk(n, t)
  if n == 0 then
    if #t % 3 == 0 then t[#t + 1] = tostring(n) else t[#t + 1] = n end
    return #t
  end
  return walk(n - 1, t) + walk(n - 1, t)
end
local s5 = 0
for i = 1, 30 do s5 = s5 + walk(6, {}) end
print("walk", s5)

-- (6) a count hook installed from inside the recursion after the traces formed (the poll hands over; the counts must be the interpreter's)
local hooks, armed, calls = 0, false, 0
local function deep(n)
  local v = n + 1
  if n == 0 then return v end
  if n == 5 and not armed then
    calls = calls + 1
    if calls == 300 then armed = true debug.sethook(function() hooks = hooks + 1 end, "", 7) end
  end
  return deep(n - 1) + v
end
local s6 = 0
for i = 1, 400 do s6 = s6 + deep(8) end
debug.sethook()
print("hook", s6, hooks)

-- (7) errors from depth after the links formed
local function err(n, at)
  local x = n * 2
  if n == at then error({ x = x }) end
  if n == 0 then return x end
  return err(n - 1, at) + x
end
local s7 = 0
for i = 1, 80 do
  local ok, e = pcall(err, 10, (i % 7 == 0) and 3 or -1)
  s7 = s7 + (ok and e or e.x)
end
print("err", s7)
