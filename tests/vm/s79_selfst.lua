-- Section 79.11 (`selfst`): array stores of the value the element already holds are dropped. (1) swaps with equal indices (permute's
-- leaf level); (2) `a[i] = a[i]` around stores that may alias (another table that is the same table at run time, another index that is the same
-- index at run time); (3) a store of a changed value after a load (must stay); (4) collectables and nil elements, __newindex on a table whose
-- element is nil, full collections and weak values.
local function swap(v, i, j)
  local tmp = v[i]
  v[i] = v[j]
  v[j] = tmp
end
local function run1(n)
  local v = { 1, 2, 3, 4, 5, 6, 7, 8 }
  local s = 0
  for k = 1, n do
    local i = k % 8 + 1
    local j = (k * 3) % 8 + 1
    if k % 4 == 0 then j = i end
    swap(v, i, j)
    swap(v, j, j)
    s = s + v[i] * k
  end
  return s, table.concat(v, ",")
end
for r = 1, 3 do print("swap", r, run1(500 + r)) end

local function run2(a, b, n)
  local s = 0
  for k = 1, n do
    local i = k % 5 + 1
    local j = (k + (k % 3)) % 5 + 1
    local x = a[i]
    b[i] = k          -- b may be a
    a[i] = x
    local y = a[i]
    a[j] = y + 1      -- j may be i
    a[i] = y
    s = s + a[i] + b[i] + a[j]
  end
  return s
end
for r = 1, 3 do
  local a = { 1, 2, 3, 4, 5 }
  local b = (r == 2) and a or { 9, 9, 9, 9, 9 }
  print("alias", r, run2(a, b, 400), table.concat(a, ","), table.concat(b, ","))
end

local function run3(a, n)
  local s = 0
  for k = 1, n do
    local i = k % 6 + 1
    local x = a[i]
    a[i] = x + (k % 2)
    a[i] = a[i]
    s = s + a[i]
  end
  return s
end
for r = 1, 3 do
  local a = { 1, 2, 3, 4, 5, 6 }
  print("changed", r, run3(a, 300), table.concat(a, ","))
end

local hits = 0
local mt = { __newindex = function(t, k, v) hits = hits + 1; rawset(t, k, v) end }
local weak = setmetatable({}, { __mode = "v" })
local function run4(n)
  local objs = {}
  for k = 1, 8 do objs[k] = { k } end
  local holes = setmetatable({ 1, nil, 3, nil, 5, nil, 7, nil }, mt)
  local s = 0
  for k = 1, n do
    local i = k % 8 + 1
    local o = objs[i]
    objs[i] = o
    weak[i] = o
    local h = holes[i]
    holes[i] = h
    if k % 100 == 0 then collectgarbage() end
    s = s + o[1] + (holes[i] or 0)
  end
  return s
end
for r = 1, 3 do
  local s = run4(600)
  collectgarbage()
  local live = 0
  for _ in pairs(weak) do live = live + 1 end
  print("gc", r, s, hits, live <= 8)
end
