-- Container libraries in the style of lua-lru (doubly linked list + hash, __pairs-less iterator closures),
-- binaryheap.lua (Tieske: comparator closure, payload/value split, reverse lookup) and a deque (pl.Deque-ish),
-- workload: cache simulation + event scheduling + sliding-window maximum.
local lru = {}
function lru.new(max_size, max_bytes)
  local size, bytes_used = 0, 0
  local map = {}
  local VALUE, PREV, NEXT, KEY, BYTES = 1, 2, 3, 4, 5
  local newest, oldest
  local removed_tuple
  local function del(key, tuple)
    map[key] = nil
    local prev, nxt = tuple[PREV], tuple[NEXT]
    if prev then prev[NEXT] = nxt end
    if nxt then nxt[PREV] = prev end
    if newest == tuple then newest = prev end
    if oldest == tuple then oldest = nxt end
    size = size - 1
    bytes_used = bytes_used - (tuple[BYTES] or 0)
    removed_tuple = tuple
  end
  local function makeFreeSpace(bytes)
    while size + 1 > max_size or (max_bytes and bytes_used + bytes > max_bytes) do
      assert(oldest, "not enough storage for cache")
      del(oldest[KEY], oldest)
    end
  end
  local function get(_, key)
    local tuple = map[key]
    if not tuple then return nil end
    -- move to newest
    if tuple ~= newest then
      local prev, nxt = tuple[PREV], tuple[NEXT]
      if prev then prev[NEXT] = nxt end
      if nxt then nxt[PREV] = prev end
      if oldest == tuple then oldest = nxt end
      tuple[PREV] = newest
      tuple[NEXT] = nil
      newest[NEXT] = tuple
      newest = tuple
    end
    return tuple[VALUE]
  end
  local function set(_, key, value, bytes)
    local tuple = map[key]
    if tuple then del(key, tuple) end
    if value ~= nil then
      bytes = max_bytes and (bytes or #value) or 0
      makeFreeSpace(bytes)
      local tuple1 = removed_tuple or {}
      map[key] = tuple1
      tuple1[VALUE] = value
      tuple1[KEY] = key
      tuple1[BYTES] = max_bytes and bytes
      size = size + 1
      bytes_used = bytes_used + bytes
      tuple1[NEXT] = nil
      tuple1[PREV] = newest
      if newest then newest[NEXT] = tuple1 end
      newest = tuple1
      if not oldest then oldest = tuple1 end
    end
    removed_tuple = nil
  end
  local function delete(_, key) return set(_, key, nil) end
  local function mynext(_, prev_key)
    local tuple
    if prev_key then tuple = map[prev_key][NEXT] else tuple = oldest end
    if tuple then return tuple[KEY], tuple[VALUE] end
  end
  local function lru_pairs() return mynext, nil, nil end
  local mt = {
    __index = { get = get, set = set, delete = delete, pairs = lru_pairs, size = function() return size end },
    __pairs = lru_pairs,
  }
  return setmetatable({}, mt)
end

-- binary heap
local floor = math.floor
local heap = {}
heap.__index = heap
local function basicHeap(swap, erase, lt)
  local h = setmetatable({ values = {}, payloads = {}, reverse = {}, lt = lt }, heap)
  return h
end
function heap:bubbleUp(pos)
  local values = self.values
  while pos > 1 do
    local parent = floor(pos / 2)
    if not self.lt(values[pos], values[parent]) then break end
    self:swap(parent, pos)
    pos = parent
  end
end
function heap:sinkDown(pos)
  local values = self.values
  local last = #values
  while true do
    local min = pos
    local child = 2 * pos
    for c = child, child + 1 do
      if c <= last and self.lt(values[c], values[min]) then min = c end
    end
    if min == pos then break end
    self:swap(pos, min)
    pos = min
  end
end
function heap:swap(a, b)
  local pl, v, rev = self.payloads, self.values, self.reverse
  pl[a], pl[b] = pl[b], pl[a]
  v[a], v[b] = v[b], v[a]
  rev[pl[a]] = a
  rev[pl[b]] = b
end
function heap:insert(value, payload)
  assert(self.reverse[payload] == nil, "duplicate payload")
  local pos = #self.values + 1
  self.reverse[payload] = pos
  self.values[pos] = value
  self.payloads[pos] = payload
  self:bubbleUp(pos)
end
function heap:peek() return self.payloads[1], self.values[1] end
function heap:pop()
  local values = self.values
  if #values == 0 then return nil end
  local pl, v = self.payloads[1], values[1]
  self:remove(pl)
  return pl, v
end
function heap:remove(payload)
  local pos = self.reverse[payload]
  if not pos then return end
  local last = #self.values
  if pos < last then
    self:swap(pos, last)
    self.values[last], self.payloads[last] = nil, nil
    self.reverse[payload] = nil
    self:bubbleUp(pos)
    self:sinkDown(pos)
  else
    self.values[last], self.payloads[last] = nil, nil
    self.reverse[payload] = nil
  end
end
function heap:update(payload, newValue)
  local pos = self.reverse[payload]
  self.values[pos] = newValue
  self:bubbleUp(pos)
  self:sinkDown(pos)
end
function heap.minUnique(lt) return basicHeap(nil, nil, lt or function(a, b) return a < b end) end

-- deque
local Deque = {}
Deque.__index = Deque
function Deque.new() return setmetatable({ first = 0, last = -1 }, Deque) end
function Deque:push_back(v) local l = self.last + 1; self.last = l; self[l] = v end
function Deque:push_front(v) local f = self.first - 1; self.first = f; self[f] = v end
function Deque:pop_back() local l = self.last; if l < self.first then return nil end local v = self[l]; self[l] = nil; self.last = l - 1; return v end
function Deque:pop_front() local f = self.first; if f > self.last then return nil end local v = self[f]; self[f] = nil; self.first = f + 1; return v end
function Deque:peek_back() return self[self.last] end
function Deque:peek_front() return self[self.first] end
function Deque:len() return self.last - self.first + 1 end

-- Workload
local N = tonumber(arg and arg[1]) or 12
local seed = 7
local function rnd(n) seed = (seed * 16807) % 2147483647; return floor(seed / 65536) % n end

local hits, misses, evsum, winsum = 0, 0, 0, 0
for r = 1, N do
  -- 1. LRU cache with zipf-ish keys
  local cache = lru.new(500, 20000)
  for i = 1, 40000 do
    local key = rnd(100) < 80 and ("k" .. rnd(400)) or ("k" .. rnd(5000))
    local v = cache:get(key)
    if v then hits = hits + 1 else misses = misses + 1; cache:set(key, "value:" .. key) end
    if i % 997 == 0 then cache:delete(key) end
  end
  local cnt = 0
  for k, v in cache:pairs() do cnt = cnt + #v end
  hits = hits + cnt % 7
  -- 2. event scheduler with a heap (timers, cancellation, rescheduling)
  local h = heap.minUnique()
  local events = {}
  for i = 1, 3000 do
    local ev = { id = i, period = 1 + rnd(50) }
    events[i] = ev
    h:insert(rnd(1000), ev)
  end
  local now, fired = 0, 0
  while fired < 20000 do
    local ev, t = h:pop()
    now = t
    fired = fired + 1
    evsum = (evsum + ev.id * (t % 13)) % 1000000007
    if ev.id % 5 ~= 0 or fired < 10000 then h:insert(t + ev.period, ev) end
    if fired % 100 == 0 then
      local victim = events[1 + rnd(3000)]
      if h.reverse[victim] then h:update(victim, now + 1) end
    end
  end
  -- 3. sliding window maximum with a deque
  local arr = {}
  for i = 1, 30000 do arr[i] = rnd(100000) end
  local dq = Deque.new()
  local K = 64
  for i = 1, #arr do
    while dq:len() > 0 and arr[dq:peek_back()] <= arr[i] do dq:pop_back() end
    dq:push_back(i)
    if dq:peek_front() <= i - K then dq:pop_front() end
    if i >= K then winsum = (winsum + arr[dq:peek_front()]) % 1000000007 end
  end
end
print("hits", hits, "misses", misses, "evsum", evsum, "winsum", winsum)
