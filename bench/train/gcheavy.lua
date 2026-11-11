-- PGO training (scripts/train.sh): collector-heavy work in both modes: short-lived small tables and closures
-- (young collections), a growing live structure (major collections, promotion, barriers), weak tables,
-- finalizers, strings that die young, mode switches. Deterministic (fixed seed, no clocks).
math.randomseed(42)
local sink = {}

local function churn(n)
  local acc = 0
  for i = 1, n do
    local t = { x = i, y = i + 1, z = i + 2 }
    local a = { i, i, i }
    local f = function() return t.x + a[1] end
    sink[1] = f
    sink[2] = { t, a }
    acc = acc + f()
  end
  return acc
end

local function tree(depth)
  if depth == 0 then return { v = 1 } end
  return { l = tree(depth - 1), r = tree(depth - 1), v = depth }
end
local function count(t) if not t.l then return 1 end return 1 + count(t.l) + count(t.r) end

local function growing(n)
  local live, total = {}, 0
  for i = 1, n do
    live[#live + 1] = { id = i, name = "obj" .. i, tags = { i % 7, i % 11 } }
    if i % 3 == 0 then live[(i * 7919) % #live + 1] = { id = -i } end -- overwrite old slots with young objects
    if i % 500 == 0 then total = total + count(tree(8)) end
  end
  return total + #live
end

local function weak(n)
  local wk = setmetatable({}, { __mode = "k" })
  local wv = setmetatable({}, { __mode = "v" })
  local keep = {}
  for i = 1, n do
    local k = { i }
    wk[k] = i
    wv[i] = { i }
    if i % 5 == 0 then keep[#keep + 1] = k end
  end
  collectgarbage()
  local c = 0
  for _ in pairs(wk) do c = c + 1 end
  return c
end

local fin = 0
local function finalizers(n)
  for i = 1, n do
    setmetatable({ i }, { __gc = function() fin = fin + 1 end })
  end
  collectgarbage()
  return fin
end

local function strings(n)
  local acc = 0
  for i = 1, n do
    local s = "key" .. i .. ":" .. (i * 3)
    acc = acc + #s + #tostring(i * 0.5)
    sink[3] = s
  end
  return acc
end

local function phase()
  print(churn(300000), growing(40000), weak(20000), finalizers(2000), strings(150000))
end

collectgarbage("generational")
phase()
collectgarbage("incremental")
phase()
collectgarbage("generational")
collectgarbage("param", "minormul", 50)
phase()
collectgarbage("step", 0)
collectgarbage("collect")
print(collectgarbage("count") > 0)
