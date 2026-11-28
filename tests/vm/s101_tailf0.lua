-- Section 101.1 (`tailf0`): a side trace whose root frame tail-calls a Lua closure before any pop (TAILF on the entry frame), and a root
-- `return setmetatable(t, mt)` followed by the pop of the frame. The shapes: a loop root whose exit path ends in `return vec(...)` (the raytrace
-- sky path), called from two call sites (the pop's site guard fails for one of them), a tail callee whose fields change type between rounds
-- (exits after the TAILF land in the callee's code), a tail callee with fewer / more parameters, errors with levels and a traceback inside the
-- tail-called function, `istailcall`, and a metatable with `__gc` / `__metatable`.

local V = {}
V.__index = V
local function vec(x, y, z) return setmetatable({ x = x, y = y, z = z }, V) end
local function vec2(x, y) return setmetatable({ x = x, y = y, z = 0 }, V) end
local function vecn(x, y, z, w) local t = setmetatable({ x = x, y = y, z = z }, V) t.w = w return t end

local objs = { 3, 5, 7, 11 }
local scale = 1

-- the sky path: the loop finds no match for most keys and the frame tail-calls `vec`
local function find(k, depth)
  local best = nil
  for i = 1, #objs do
    if objs[i] == k then best = i end
  end
  if not best then
    local q = k * 0.5 * scale
    if depth == 1 then return vec2(q, q + 1) end
    if depth == 2 then return vecn(q, q + 1, q + 2, depth) end
    return vec(q, q + 1, 1.0)
  end
  return vec(best, 0, 0)
end

local function sum_a(n, depth)
  local s = 0
  for i = 1, n do
    local r = find(i % 13, depth)
    s = s + r.x + r.y + r.z
  end
  return s
end

local function sum_b(n, depth)
  local s = 0
  for i = 1, n do
    local r = find(i % 17, depth)
    s = s + r.x * 2 - r.y
  end
  return s
end

for round = 1, 6 do
  if round == 4 then scale = 3 end -- integer factor: the fields change type
  if round == 5 then objs[2] = 6.0 end
  local a = sum_a(3000, round % 3)
  local b = sum_b(3000, round % 3)
  print("round", round, a, b, math.type(a), math.type(b))
end

-- the callee's own fields used after the pop: types vary per call
local function mk(k)
  for i = 1, 2 do
    if objs[i] == k then return vec(i, i, i) end
  end
  return vec(k // 2, k / 2, k % 3)
end
local acc = {}
for i = 1, 4000 do
  local r = mk(i % 9)
  acc[#acc + 1] = math.type(r.x) .. math.type(r.y) .. math.type(r.z)
  if #acc > 8 then table.remove(acc, 1) end
end
print(table.concat(acc, ","))

-- errors inside the tail-called function (level 1 and 2), traceback and istailcall
local W = {}
W.__index = W
local function bad(x, mode)
  if x > 3500 then
    if mode == 1 then error("bad value " .. x) end
    if mode == 2 then error("caller's fault", 2) end
    local info = debug.getinfo(1, "t")
    return setmetatable({ tail = info.istailcall, tb = (debug.traceback("tb", 1):find("tail calls", 1, true) ~= nil) }, W)
  end
  return setmetatable({ x = x }, W)
end
local function via(x, mode)
  for i = 1, 2 do
    if objs[i] == x then return nil end
  end
  return bad(x, mode)
end
for mode = 0, 2 do
  local ok, err = pcall(function()
    local last
    for i = 1, 4000 do last = via(i, mode) end
    return last
  end)
  if ok then
    print("mode", mode, ok, err and err.tail, err and err.tb)
  else
    print("mode", mode, ok, (tostring(err):gsub("^.-:%d+: ", "")))
  end
end

-- finalizers and protected metatables reached through the native tail
collectgarbage("stop")
local G = { __gc = function(o) end }
local P = { __metatable = "locked" }
local function mkg(i, kind)
  for j = 1, 2 do
    if objs[j] == i then return nil end
  end
  if kind == 1 then return setmetatable({ i = i }, G) end
  return setmetatable({ i = i }, P)
end
local cnt = 0
for i = 1, 3000 do
  local o = mkg(i, 1 + i % 2)
  if o and getmetatable(o) == "locked" then cnt = cnt + 1 end
end
collectgarbage("restart")
collectgarbage()
print("locked", cnt)
local ok, err = pcall(function()
  local o = mkg(10, 2)
  return setmetatable(o, {})
end)
print("protected", ok, (tostring(err):gsub("^.-:%d+: ", "")))

-- a function root entered from C (`gsub`) whose final exit is its `TAILCALL`: the side trace starting there replaces the frame and stitches at
-- a C call inside the callee without popping (refused with `tailf0` bit 2)
local clock = os.clock
local function slow(x)
  local a = x * 3 + 1
  local b = a % 7 + x // 2
  local c = (a + b) * 2 - x
  local d = c + a - b
  local t = clock()
  return d + (t >= 0 and 1 or 0)
end
local function cb(ch)
  local v = string.byte(ch)
  return slow(v)
end
local text = string.rep("abcdefghij", 300)
local out = string.gsub(text, ".", cb)
print("gsub", #out, out:sub(1, 20))
