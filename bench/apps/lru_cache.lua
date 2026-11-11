-- lru_cache: simulation of an LRU cache built from closures and metatables (doubly linked list of entries in a
-- hash map; the cache object exposes get/put through __index/__newindex/__call metamethods, evictions run
-- user callbacks closed over statistics counters).  A skewed pseudo-random key stream drives several caches
-- of different capacity that compute missing values with memoised closures.  Plain Lua 5.1-compatible.
-- Output: hits, misses, evictions and a checksum of the values handed out.
local N = tonumber(arg and arg[1]) or 90000
local setmetatable, rawget, rawset = setmetatable, rawget, rawset

local Cache = {}
Cache.__index = function(self, k)
  local m = Cache[k]
  if m ~= nil then return m end
  return self:get(k)          -- cache[key] reads through the cache (nil on miss)
end
Cache.__newindex = function(self, k, v) self:put(k, v) end
Cache.__call = function(self, k, compute)  -- cache(key, compute): memoised lookup
  local v = self:get(k)
  if v == nil then
    v = compute(k)
    self:put(k, v)
  end
  return v
end

local function newCache(capacity, onEvict)
  local self = setmetatable({}, Cache)
  -- internal state lives in upvalues (closures), not in the object, so that __index/__newindex stay simple
  local map, head, tail, size = {}, nil, nil, 0
  local hits, misses, evictions = 0, 0, 0
  local function unlink(e)
    if e.prev then e.prev.next = e.next else head = e.next end
    if e.next then e.next.prev = e.prev else tail = e.prev end
    e.prev, e.next = nil, nil
  end
  local function pushFront(e)
    e.next = head
    if head then head.prev = e end
    head = e
    if not tail then tail = e end
  end
  rawset(self, "get", function(_, k)
    local e = map[k]
    if e then
      hits = hits + 1
      if e ~= head then unlink(e); pushFront(e) end
      return e.value
    end
    misses = misses + 1
    return nil
  end)
  rawset(self, "put", function(_, k, v)
    local e = map[k]
    if e then
      e.value = v
      if e ~= head then unlink(e); pushFront(e) end
      return
    end
    e = { key = k, value = v }
    map[k] = e
    pushFront(e)
    size = size + 1
    if size > capacity then
      local victim = tail
      unlink(victim)
      map[victim.key] = nil
      size = size - 1
      evictions = evictions + 1
      if onEvict then onEvict(victim.key, victim.value) end
    end
  end)
  rawset(self, "size", function() return size end)
  rawset(self, "stats", function() return hits, misses, evictions end)
  return self
end

-- key stream: skewed (product of two uniforms) over 20000 keys, strings and integers mixed
local seed = 7
local function rnd(n) seed = (seed * 16807) % 2147483647 return seed % n end
local function nextKey()
  local a, b = rnd(150), rnd(150)
  local k = a * b % 20000
  if k % 5 == 0 then return "key" .. k end
  return k
end

local evictedSum = 0
local caches = {
  newCache(64, function(k, v) evictedSum = evictedSum + v end),
  newCache(512, function(k, v) evictedSum = evictedSum + v % 7 end),
  newCache(4096),
}
local computed = 0
local function compute(k)
  computed = computed + 1
  if type(k) == "string" then return #k * 13 end
  return k % 1013 + 1
end

local check = 0
for i = 1, N do
  local k = nextKey()
  local c = caches[i % 3 + 1]
  if i % 11 == 0 then
    c[k] = i % 100          -- __newindex
  else
    check = (check + c(k, compute)) % 1000003   -- __call
  end
  if i % 97 == 0 then
    local v = c[k]                              -- __index
    if v then check = (check + v) % 1000003 end
  end
end
local hits, misses, ev = 0, 0, 0
for i = 1, 3 do
  local h, m, e = caches[i]:stats()
  hits, misses, ev = hits + h, misses + m, ev + e
end
print("lru_cache", hits, misses, ev, computed, evictedSum, check)
