-- Random round trips (fixed seed, identical generator in both engines): decode(encode(v)) must show the same value,
-- and the bytes are printed for the shapes whose layout does not depend on hash iteration order (dense arrays of
-- scalars/arrays, single-key maps). Both engines must print identical lines.
local C = dofile("_common.lua")
local hex, show = C.hex, C.show
local buf = require("string.buffer")
local rnd = C.rng(12345)

local function str()
  local n = rnd(6) == 1 and rnd(300) or rnd(12)
  local t = {}
  for i = 1, n do t[i] = string.char(rnd(256) - 1) end
  return table.concat(t)
end

local function scalar()
  local k = rnd(9)
  if k == 1 then return true elseif k == 2 then return false
  elseif k == 3 then return rnd(1000) - 500
  elseif k == 4 then return (rnd(1000) - 500) / 8
  elseif k == 5 then return rnd(2 ^ 30) * rnd(2 ^ 20)
  elseif k == 6 then return -rnd(2 ^ 30)
  elseif k == 7 then return str()
  elseif k == 8 then return ({ 1e100, -1e-100, 2 ^ 53, -2 ^ 53, 0.1 })[rnd(5)]
  else return "key" .. rnd(50) end
end

local function array(depth)
  local t = {}
  for i = 1, rnd(12) do
    t[i] = (depth > 0 and rnd(4) == 1) and array(depth - 1) or scalar()
  end
  return t
end

local function map(depth)
  local t = {}
  for i = 1, rnd(8) do
    local k
    local kk = rnd(5)
    if kk == 1 then k = rnd(100) + 20 elseif kk == 2 then k = rnd(1000) / 4 + 0.125 elseif kk == 3 then k = rnd(2) == 1
    else k = "k" .. rnd(30) end
    t[k] = (depth > 0 and rnd(4) == 1) and (rnd(2) == 1 and array(depth - 1) or map(depth - 1)) or scalar()
  end
  if rnd(3) == 1 then for i = 1, rnd(6) do t[i] = scalar() end end
  return t
end

local bad = 0
for i = 1, 150 do
  local kind = rnd(4)
  local v, bytes
  if kind == 1 then v = array(2) bytes = true
  elseif kind == 2 then v = map(2)
  elseif kind == 3 then v = { [rnd(2) == 1 and "only" or "other"] = scalar() } bytes = true
  else v = scalar() bytes = true end
  local s = buf.encode(v)
  local back = buf.decode(s)
  local a, b = show(v), show(back)
  if a ~= b then bad = bad + 1 end
  print(i, kind, a == b, bytes and #s or "-", bytes and hex(s) or "-", b)
end
print("mismatches", bad)
-- the same values through a buffer object (encode appends several, decode reads them back in order)
local x = buf.new()
local vals = {}
for i = 1, 60 do vals[i] = rnd(2) == 1 and array(1) or map(1); x:encode(vals[i]) end
local ok = 0
for i = 1, 60 do if show(x:decode()) == show(vals[i]) then ok = ok + 1 end end
print("stream", ok, #x)
-- strings of many lengths
for _, n in ipairs { 0, 1, 30, 31, 32, 223, 224, 225, 1000, 8159, 8160, 8161, 70000 } do
  local s = ("ab"):rep(n)
  local r = buf.decode(buf.encode(s))
  print("len", n, r == s, #buf.encode({ s, s }))
end
