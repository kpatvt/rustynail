-- Memoization helpers (single/multi-arg via nested tables or string keys), an LRU cache with a
-- doubly linked list, applied to recursive problems and an "expensive lookup" workload. small: N=2
local N = tonumber(arg and arg[1]) or 220

local seed = 2718
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- memoize: single argument ----------------------------------------------------------------
local function memoize(f)
  local cache = {}
  return function(x)
    local v = cache[x]
    if v == nil then
      v = f(x)
      cache[x] = v
    end
    return v
  end
end

-- memoize: multiple arguments through nested tables
local NIL = {}
local function memoizeN(f)
  local root = {}
  local hits, misses = 0, 0
  local function wrapper(...)
    local node = root
    local n = select('#', ...)
    for i = 1, n do
      local k = select(i, ...)
      if k == nil then k = NIL end
      local nxt = node[k]
      if not nxt then nxt = {}; node[k] = nxt end
      node = nxt
    end
    local v = node.value
    if v == nil then
      misses = misses + 1
      v = f(...)
      node.value = v
    else
      hits = hits + 1
    end
    return v
  end
  return wrapper, function() return hits, misses end
end

-- memoize: multiple arguments through a joined string key
local function memoizeKey(f)
  local cache = {}
  return function(...)
    local key = table.concat({ ... }, ",")
    local v = cache[key]
    if v == nil then
      v = f(...)
      cache[key] = v
    end
    return v
  end
end

-- LRU cache ---------------------------------------------------------------------------------------
local LRU = {}
LRU.__index = LRU

function LRU.new(capacity)
  local head, tail = {}, {}
  head.next, tail.prev = tail, head
  return setmetatable({ cap = capacity, size = 0, map = {}, head = head, tail = tail,
    hits = 0, misses = 0, evictions = 0 }, LRU)
end

function LRU:_unlink(node)
  node.prev.next = node.next
  node.next.prev = node.prev
end

function LRU:_pushFront(node)
  node.next = self.head.next
  node.prev = self.head
  self.head.next.prev = node
  self.head.next = node
end

function LRU:get(key)
  local node = self.map[key]
  if not node then
    self.misses = self.misses + 1
    return nil
  end
  self.hits = self.hits + 1
  self:_unlink(node)
  self:_pushFront(node)
  return node.value
end

function LRU:put(key, value)
  local node = self.map[key]
  if node then
    node.value = value
    self:_unlink(node)
    self:_pushFront(node)
    return
  end
  if self.size >= self.cap then
    local last = self.tail.prev
    self:_unlink(last)
    self.map[last.key] = nil
    self.size = self.size - 1
    self.evictions = self.evictions + 1
  end
  node = { key = key, value = value }
  self.map[key] = node
  self:_pushFront(node)
  self.size = self.size + 1
end

function LRU:keys()
  local out, node = {}, self.head.next
  while node ~= self.tail do
    out[#out + 1] = node.key
    node = node.next
  end
  return out
end

-- Recursive problems --------------------------------------------------------------------------------
local fib
fib = memoize(function(n)
  if n < 2 then return n end
  return (fib(n - 1) + fib(n - 2)) % 1000000007
end)

local partitions, partStats
partitions, partStats = memoizeN(function(n, k)
  if n == 0 then return 1 end
  if n < 0 or k == 0 then return 0 end
  return (partitions(n - k, k) + partitions(n, k - 1)) % 1000000007
end)

local gridPaths
gridPaths = memoizeKey(function(r, c, blockedMod)
  if r < 0 or c < 0 then return 0 end
  if (r * 7 + c * 3) % blockedMod == 0 and r + c > 0 then return 0 end
  if r == 0 and c == 0 then return 1 end
  return (gridPaths(r - 1, c, blockedMod) + gridPaths(r, c - 1, blockedMod)) % 1000000007
end)

local function editDistance(a, b)
  local cache = {}
  local function go(i, j)
    if i == 0 then return j end
    if j == 0 then return i end
    local key = i * 1000 + j
    local v = cache[key]
    if v then return v end
    if a:byte(i) == b:byte(j) then
      v = go(i - 1, j - 1)
    else
      v = 1 + math.min(go(i - 1, j), go(i, j - 1), go(i - 1, j - 1))
    end
    cache[key] = v
    return v
  end
  return go(#a, #b)
end

local function collatzLen(n, cache)
  if n == 1 then return 1 end
  local v = cache[n]
  if v then return v end
  if n % 2 == 0 then v = 1 + collatzLen(n / 2, cache) else v = 1 + collatzLen(3 * n + 1, cache) end
  cache[n] = v
  return v
end

-- Expensive lookup workload ---------------------------------------------------------------------------
local expensiveCalls = 0
local function expensiveLookup(key)
  expensiveCalls = expensiveCalls + 1
  local h = key
  for _ = 1, 20 do h = (h * 31 + 7) % 1000003 end
  return h
end

local function zipf()
  local r = rnd(100)
  if r <= 50 then return rnd(20)
  elseif r <= 80 then return rnd(200)
  else return rnd(5000) end
end

-- Main -------------------------------------------------------------------------------------------------
print("fib(80) mod p = " .. fib(80) .. ", fib(900) mod p = " .. fib(900))

local pn = 40 + N * 3
print(string.format("partitions(%d) = %d", pn, partitions(pn, pn)))
local hits, misses = partStats()
print(string.format("partition memo hits=%d misses=%d", hits, misses))

local total = 0
for m = 1, math.max(2, math.floor(N / 3)) do
  total = (total + gridPaths(20 + m, 20 + m, 11 + m)) % 1000000007
end
print("grid path total = " .. total)

local words = { "kitten", "sitting", "saturday", "sunday", "intention", "execution", "algorithm", "altruistic" }
local dsum = 0
for rep = 1, N do
  for i = 1, #words do
    for j = 1, #words do dsum = dsum + editDistance(words[i], words[j]) end
  end
end
print("edit distance sum = " .. dsum)

local cc, bestN, bestLen = {}, 1, 1
for n = 1, N * 400 do
  local len = collatzLen(n, cc)
  if len > bestLen then bestN, bestLen = n, len end
end
print(string.format("longest collatz below %d: n=%d len=%d", N * 400 + 1, bestN, bestLen))

local lru = LRU.new(64)
local sum = 0
for i = 1, N * 400 do
  local key = zipf()
  local v = lru:get(key)
  if v == nil then
    v = expensiveLookup(key)
    lru:put(key, v)
  end
  sum = (sum + v) % 1000003
end
local keys = lru:keys()
print(string.format("lru: size=%d hits=%d misses=%d evictions=%d expensive=%d sum=%d",
  lru.size, lru.hits, lru.misses, lru.evictions, expensiveCalls, sum))
print("most recent keys: " .. table.concat(keys, ",", 1, math.min(8, #keys)))

-- consistency check: the list and the map agree
local count = 0
for _ in pairs(lru.map) do count = count + 1 end
print("list/map consistent: " .. tostring(count == #keys and count == lru.size))
