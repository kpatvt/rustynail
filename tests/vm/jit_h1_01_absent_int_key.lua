-- H1: integer-key reads that miss (absent / nil in the array part / hash part), with and without metatables, in loops hot
-- enough for tier 1 and the trace recorder; shapes change between iterations (metatable added later, __index, array
-- growth and shrink, negative / zero / huge keys, float keys).
local out = {}
local function s(v) return tostring(v) end
local function emit(...)
  local n = select("#", ...)
  local p = {...}
  for i = 1, n do p[i] = s(p[i]) end
  out[#out + 1] = table.concat(p, ",", 1, n)
end

-- 1. leaf read on empty / non-empty tables (binary_trees shape)
local function item(t)
  if t[1] then return 1 + item(t[1]) + item(t[2]) end
  return 1
end
local function bottom(d)
  if d > 0 then return {bottom(d - 1), bottom(d - 1)} end
  return {}
end
local total = 0
for i = 1, 60 do total = total + item(bottom(6)) end
emit("tree", total)

-- 2. metatable added later to the leaf shape, then removed
local mt = {__index = function(_, k) return k * 10 end}
local mt2 = {__index = {[1] = "cls", [5] = "five"}}
local acc = {}
for i = 1, 600 do
  local t = {}
  local phase = (i // 50) % 4
  if phase == 1 then setmetatable(t, mt) elseif phase == 2 then setmetatable(t, mt2) end
  local a, b, c = t[1], t[5], t[-3]
  acc[#acc + 1] = s(a) .. s(b) .. s(c)
  if phase == 3 then setmetatable(t, nil); acc[#acc + 1] = s(t[2]) end
end
emit("mt", #acc, acc[1], acc[60], acc[110], acc[160], acc[#acc])

-- 3. metatable added to the same table between reads
local shared = {}
local seen = {}
for i = 1, 500 do
  if i == 250 then setmetatable(shared, {__index = function(_, k) return "mm" .. k end}) end
  if i == 400 then setmetatable(shared, nil) end
  seen[#seen + 1] = s(shared[3])
end
emit("late", seen[1], seen[249], seen[250], seen[399], seen[400], seen[500])

-- 4. array growth / shrink between reads
local g = {}
local r = {}
for i = 1, 400 do
  local k = i % 20 + 1
  r[#r + 1] = s(g[k])
  if i % 7 == 0 then g[k] = i end
  if i % 11 == 0 then g[k] = nil end
  if i % 97 == 0 then g = {} end
  if i % 53 == 0 then for j = 1, 30 do g[j] = j end end
  if i % 61 == 0 then for j = 1, 30 do g[j] = nil end end
end
emit("grow", #r, table.concat(r, "", 1, 100), table.concat(r, "", 301, 400))

-- 5. keys: zero, negative, huge, float (integral and not), hash-part integers
local t = {10, 20, 30, [0] = "z", [-1] = "m", [1 << 40] = "big", [math.maxinteger] = "max", [math.mininteger] = "min", [100] = "h"}
local keys = {1, 3, 4, 0, -1, 1 << 40, math.maxinteger, math.mininteger, 100, 101, 2.0, 2.5, 3.0, 1e100, -0.0, 7}
local kr = {}
for i = 1, 400 do
  local k = keys[i % #keys + 1]
  kr[#kr + 1] = s(t[k])
end
emit("keys", table.concat(kr, "|", 1, 40), #kr)
local nk = {}
for i = 1, 400 do
  local k = i % 5 - 2
  nk[#nk + 1] = s(t[k])
  local e = {}
  nk[#nk + 1] = s(e[k]) .. s(e[1 << 62]) .. s(e[math.mininteger])
end
emit("small", table.concat(nk, "", 1, 30))

-- 6. __index chain on a table with integer keys; rawget unaffected
local base = {[1] = "b1", [2] = "b2"}
local mid = setmetatable({[2] = "m2"}, {__index = base})
local leafs = {}
for i = 1, 300 do
  local l = (i % 3 == 0) and setmetatable({}, {__index = mid}) or {}
  leafs[#leafs + 1] = s(l[1]) .. s(l[2]) .. s(l[3]) .. s(rawget(l, 1))
end
emit("chain", table.concat(leafs, ";", 1, 6), #leafs)

-- 7. non-table indexed values keep their errors
local ok, err = pcall(function() local n = 5; for i = 1, 300 do local _ = n[1] end end)
emit("err", ok, (err:gsub("^.-:%d+: ", "")))
local ok2, err2 = pcall(function() local z; return z[1] end)
emit("err2", ok2, (err2:gsub("^.-:%d+: ", "")))
local str = "abc"
emit("str", str[1], type(("x").len))

for _, l in ipairs(out) do print(l) end
