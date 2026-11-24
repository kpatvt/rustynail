-- T4 (`t4`, docs/JIT_TRACE.md section 46): the guards of a native return (the returning frame was called by a Lua frame of the recursion, at a
-- continuation site that wants one result) run at the end of the trace. A failure (a call from outside the recursion, a caller with another wanted
-- count, a caller that is a C function) exits in front of the `RETURN` with the body done: every case below returns through such a caller many times,
-- with side effects in the return path that a second run of the body would repeat (the counters), and with values of the body live in that exit.
-- Bit 1: the start snapshot of those traces (the entry guards of the body: table shapes) may get side traces.

-- (1) a left-leaning red-black tree insert (awfy cd's `put`), called by a method (never a continuation site): every insert ends with a late exit;
-- nodes are made with their keys in different orders, so the shape guards at the start of the return traces fail (bit 1's side traces)
local Tree = {}
Tree.__index = Tree
local function isRed(h) return h ~= nil and h.red end
local function rotateLeft(h)
  local x = h.right
  h.right = x.left
  x.left = h
  x.red = h.red
  h.red = true
  return x
end
local function rotateRight(h)
  local x = h.left
  h.left = x.right
  x.right = h
  x.red = h.red
  h.red = true
  return x
end
local function flip(h)
  h.red = not h.red
  h.left.red = not h.left.red
  h.right.red = not h.right.red
end
local cnt = { ret = 0, rot = 0, new = 0 }
local function put(tree, h, key, val)
  if h == nil then
    tree.size = tree.size + 1
    cnt.new = cnt.new + 1
    if key % 3 == 0 then return { key = key, value = val, red = true }
    elseif key % 3 == 1 then return { red = true, value = val, key = key }
    else local t = { value = val } t.key = key t.red = true return t end
  end
  if key < h.key then
    h.left = put(tree, h.left, key, val)
  elseif key > h.key then
    h.right = put(tree, h.right, key, val)
  else
    h.value = val
  end
  cnt.ret = cnt.ret + 1
  if isRed(h.right) and not isRed(h.left) then h = rotateLeft(h) cnt.rot = cnt.rot + 1 end
  if isRed(h.left) and isRed(h.left.left) then h = rotateRight(h) cnt.rot = cnt.rot + 1 end
  if isRed(h.left) and isRed(h.right) then flip(h) end
  return h
end
function Tree:put(key, val)
  self.root = put(self, self.root, key, val)
  self.root.red = false
end
local function height(h) if h == nil then return 0 end local a, b = height(h.left), height(h.right) return 1 + (a > b and a or b) end
local function sum(h) if h == nil then return 0 end return h.key + sum(h.left) + sum(h.right) end
for round = 1, 6 do
  local t = setmetatable({ size = 0 }, Tree)
  local k = round
  for i = 1, 300 do
    k = (k * 37 + 11) % 1009
    t:put(k, i)
  end
  print("rbt", round, t.size, height(t.root), sum(t.root), cnt.ret, cnt.rot, cnt.new)
end

-- (2) the same recursion entered through pcall (its caller is a C function: the callstatus test fails at the end) and through a coroutine body
local function depth(n, acc)
  if n == 0 then return acc end
  local v = depth(n - 1, acc + n)
  cnt.ret = cnt.ret + 1
  return v * 2 % 1000003
end
local p1 = 0
for i = 1, 200 do
  local ok, v = pcall(depth, 5 + i % 4, i)
  p1 = p1 + v
end
print("pcall", p1, cnt.ret)
local co = coroutine.wrap(function(n)
  local s = 0
  while true do
    s = s + depth(n, s % 7)
    n = coroutine.yield(s)
  end
end)
local c1 = 0
for i = 1, 200 do c1 = (c1 + co(3 + i % 5)) % 1000003 end
print("coro", c1, cnt.ret)
-- a coroutine whose body is the recursion itself (the base frame's caller is `resume`), yielding at the leaves
local function yrec(n)
  if n == 0 then return coroutine.yield(n) or 1 end
  local v = yrec(n - 1)
  cnt.ret = cnt.ret + 1
  return v + n
end
local c2 = 0
for i = 1, 120 do
  local c = coroutine.create(yrec)
  local _, a = coroutine.resume(c, 2 + i % 4)
  local _, b = coroutine.resume(c, i)
  c2 = c2 + a + b
end
print("yrec", c2, cnt.ret)

-- (3) callers inside the recursion with other wanted counts (no continuation site for them: late exits in the middle of the recursion)
local function mixed(n)
  if n <= 1 then return n end
  local a = mixed(n - 1)
  if n % 3 == 0 then
    local b, c = mixed(n - 2)
    a = a + b + (c or 0)
  else
    mixed(n - 2)
  end
  cnt.ret = cnt.ret + 1
  return a + 1
end
local m1 = 0
for i = 1, 60 do m1 = m1 + mixed(8 + i % 3) end
print("mixed", m1, cnt.ret)

-- (4) values of the body live at the exit (computed after the recursive call, read by the caller through the result and a table)
local out = {}
local function live(n, t)
  if n == 0 then return 1 end
  local r = live(n - 1, t)
  local x = r * 3 + n
  local s = "v" .. (x % 10)
  t[n] = s
  cnt.ret = cnt.ret + 1
  return x % 1000
end
local l1 = 0
for i = 1, 150 do
  l1 = l1 + live(4 + i % 5, out)
end
print("live", l1, out[1], out[2], out[3], cnt.ret)

-- (5) a return hook installed from the top-level caller between rounds and a count hook from inside the recursion
local hooks = 0
for round = 1, 3 do
  if round == 2 then debug.sethook(function() hooks = hooks + 1 end, "r") end
  local s = 0
  for i = 1, 100 do s = s + depth(4 + i % 3, i) end
  debug.sethook()
  print("hook", round, s, hooks > 0)
end
local counted = 0
local function crec(n)
  if n == 0 then
    if counted == 0 then debug.sethook(function() counted = counted + 1 end, "", 50) end
    return 0
  end
  local v = crec(n - 1)
  cnt.ret = cnt.ret + 1
  return v + n
end
local h1 = 0
for i = 1, 100 do h1 = h1 + crec(3 + i % 4) end
debug.sethook()
print("count", h1, counted > 0)

-- (6) errors and tracebacks: an error raised in the return path at the top level (after the late exit's body) and from depth
local function erec(n, bad)
  if n == 0 then return 0 end
  local v = erec(n - 1, bad)
  if bad == n then error("bad " .. n) end
  return v + n
end
local e1, e2 = 0, ""
for i = 1, 100 do
  local ok, msg = pcall(erec, 5, (i % 7 == 0) and (i % 5 + 1) or 0)
  if ok then e1 = e1 + msg else e2 = msg:gsub("^.-:%d+: ", "") end
end
print("err", e1, e2)
local function trec(n)
  -- (the frames of the recursion only: the host's own frames below the main chunk differ between hosts)
  if n == 0 then return select(2, debug.traceback("x", 1):gsub("trec'", "")) end
  local v = trec(n - 1)
  return v
end
local t1 = 0
for i = 1, 60 do t1 = t1 + trec(3 + i % 3) end
print("traceback", t1)

-- (7) deep recursion from a non-site caller (the stack grows while it descends), allocation and collections in the return path
local function deep(n)
  if n == 0 then return {} end
  local t = deep(n - 1)
  t[#t + 1] = n
  if n % 500 == 0 then collectgarbage("step") end
  return t
end
local d1 = 0
for i = 1, 6 do local t = deep(2000 + i * 300) d1 = d1 + #t + t[1] end
print("deep", d1)

-- (8) finalizers that run while the recursion returns
local fin = 0
local mt = { __gc = function() fin = fin + 1 end }
local function grec(n)
  if n == 0 then return 0 end
  local v = grec(n - 1)
  setmetatable({}, mt)
  return v + 1
end
for i = 1, 200 do grec(3 + i % 3) end
collectgarbage()
collectgarbage()
print("gc", fin > 0, cnt.ret)
