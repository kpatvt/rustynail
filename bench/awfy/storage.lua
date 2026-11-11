-- Storage: allocation of a tree of arrays (port from the public are-we-fast-yet "Storage" description):
-- buildTreeDepth(7) creates a 4-ary tree whose leaves are arrays of pseudo-random size 1..10; the benchmark
-- counts the arrays.  GC/allocation heavy.  Verification: count == 5461 per iteration.
-- Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
local N = tonumber(arg and arg[1]) or 45

local Random = {}
Random.__index = Random
local function newRandom() return setmetatable({ seed = 74755 }, Random) end
function Random:next()
  self.seed = (self.seed * 1309 + 13849) % 65536
  return self.seed
end

local count
local function buildTreeDepth(depth, random)
  count = count + 1
  if depth == 1 then
    local size = random:next() % 10 + 1
    local a = {}
    for i = 1, size do a[i] = false end
    return a
  else
    local arr = {}
    for i = 1, 4 do arr[i] = buildTreeDepth(depth - 1, random) end
    return arr
  end
end

local total, ok = 0, 0
for it = 1, N do
  local random = newRandom()
  count = 0
  local tree = buildTreeDepth(7, random)
  total = total + count
  if count == 5461 then ok = ok + 1 end
end
print("storage", ok, total)
