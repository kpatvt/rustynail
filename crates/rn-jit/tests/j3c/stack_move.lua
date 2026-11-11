-- stack growth and shrink while compiled frames are live
local function rec(n) if n == 0 then return 0 end return 1 + rec(n - 1) end
local function rec3(n, a, b, c)
  if n == 0 then return a + b + c end
  local x, y = a + 1, b * 2 % 1000
  local r = rec3(n - 1, x, y, c)
  return r + x - y
end
-- 1. growth inside a compiled loop whose locals live in registers across the growth
local function grow(n)
  local s, t, u = 0, 1.5, "k"
  local keep = {}
  for i = 1, n do
    local before = i * 3
    local r = rec(DEPTH // 4 + i * 10)
    local r3 = rec3(DEPTH // 8 + i, i, 2, 3)
    s = s + r + before + r3 % 7
    t = t * 1.0001
    keep[#keep + 1] = r
    if i % 2 == 0 then collectgarbage() end        -- shrinks the stack at the atomic phase
  end
  return s, string.format("%.6f", t), u, #keep, keep[1], keep[#keep]
end
P("grow", grow(12))
-- 2. shrink by collectgarbage() with callers' registers read afterwards
local function shrink(n)
  local a, b, c, d = 1, 2.5, "s", {}
  for i = 1, n do
    local deep = rec(DEPTH // 2)
    collectgarbage()
    collectgarbage("step", 0)
    a = a + deep % 3
    b = b + a * 0.5
    d[i] = c .. i
  end
  return a, b, c, #d, d[n]
end
P("shrink", shrink(8))
-- 3. coroutines that grow and shrink; yields from deep inside
local function worker(rounds)
  local acc = 0
  for r = 1, rounds do
    local d = rec(DEPTH // 6 + r)
    acc = acc + d
    local got = coroutine.yield(acc)
    acc = acc + (got or 0)
    if r % 2 == 0 then collectgarbage() end
  end
  return "end", acc
end
do
  local cos = {}
  for i = 1, 6 do cos[i] = coroutine.create(worker) end
  local log = {}
  for round = 1, 5 do
    for i = 1, 6 do
      local ok, a, b = coroutine.resume(cos[i], round == 1 and 4 or i * round)
      log[#log + 1] = tostring(ok) .. ":" .. tostring(a) .. ":" .. tostring(b)
    end
    collectgarbage()
  end
  P("coros", table.concat(log, " "))
end
do -- yield from depth, then the stack of the suspended coroutine is shrunk by a collection
  local co = coroutine.wrap(function()
    local function deepyield(n) if n == 0 then return coroutine.yield("bottom") end return deepyield(n - 1) + 1 end
    local s = 0
    for i = 1, 3 do s = s + deepyield(DEPTH // 5) end
    return s
  end)
  local r = {co()}
  collectgarbage() collectgarbage()
  r[#r + 1] = co(10)
  collectgarbage()
  r[#r + 1] = co(20)
  r[#r + 1] = co(30)
  P("deepyield", table.concat(r, ","))
end
-- 4. error unwinding through compiled frames
local function thrower(n, kind)
  if n == 0 then
    if kind == 1 then error("boom") elseif kind == 2 then error({code = 7}) elseif kind == 3 then local x = nil + 1
    else error(setmetatable({}, {__tostring = function() return "obj" end})) end
  end
  local r = thrower(n - 1, kind)
  return r + 1
end
for kind = 1, 4 do
  for _, d in ipairs{10, DEPTH // 3} do
    local ok, e = pcall(thrower, d, kind)
    P("throw", kind, d, ok, type(e) == "table" and (e.code or tostring(e)) or e)
  end
end
-- message handler that grows the stack
local function handler(m)
  local d = rec(DEPTH // 4)
  return tostring(m) .. "|" .. d
end
for kind = 1, 3 do
  P("xpcall", kind, xpcall(thrower, handler, 50, kind))
end
-- handler that errors and handler that overflows
P("hand-err", xpcall(thrower, function(m) error("again") end, 5, 1))
if OVF then
  P("hand-rec", xpcall(thrower, function(m) local function r() return 1 + r() end return r() end, 5, 1))
end
-- nested pcall/xpcall inside loops, with growth in the protected function
local function nested(n)
  local s = 0
  for i = 1, n do
    local ok, r = pcall(function()
      local d = rec(DEPTH // 8 + i)
      if i % 3 == 0 then error("e" .. i) end
      return d
    end)
    s = s + (ok and r or #r)
    local ok2, r2 = xpcall(function() return rec3(50, i, 2, 3) end, handler)
    s = s + r2
  end
  return s
end
P("nested", nested(15))
-- 5. stack overflow in compiled recursion: error text and recovery
if OVF then
  local function inf(n) return 1 + inf(n + 1) end
  for rep = 1, 3 do
    local ok, e = pcall(inf, 1)
    P("overflow", rep, ok, (e:gsub("%d+", "N")))
    collectgarbage()
  end
  local depth = 0
  local function count() depth = depth + 1 return 1 + count() end
  local ok, e = pcall(count)
  P("depth", depth > 1000, ok)
  -- recovery: deep but legal recursion works again, in a loop, after the overflow
  local s = 0
  for i = 1, 5 do s = s + rec(DEPTH) end
  P("recovered", s)
  -- overflow inside a coroutine
  local co = coroutine.create(function() return inf(1) end)
  local ok2, e2 = coroutine.resume(co)
  P("co overflow", ok2, (tostring(e2):gsub("%d+", "N")))
  -- overflow with tbc in the chain
  local function tb(n)
    local x <close> = setmetatable({}, {__close = function() depth = depth + 1 end})
    return 1 + tb(n + 1)
  end
  local before = depth
  local ok3, e3 = pcall(tb, 1)
  P("tbc overflow", ok3, (tostring(e3):gsub("%d+", "N")), depth - before > 1000)
end
return table.concat(out, "\n")
