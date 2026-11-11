-- events: string-keyed event dispatcher with callbacks: handlers registered under dotted names
-- ("net.http.request"), delivered to the exact name and to every ancestor prefix and to the "*" wildcard,
-- priorities (insertion into sorted handler lists), one-shot handlers, variadic payloads, handlers that emit further
-- events and handlers that unsubscribe themselves.  Callback heavy, string-key heavy, closures with upvalues.
-- Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
-- Output: handler invocations, sum of payload checksums and the remaining subscription count.
local N = tonumber(arg and arg[1]) or 85000
local insert, remove = table.insert, table.remove
local find, sub, format = string.find, string.sub, string.format
local select = select

local Bus = {}
Bus.__index = Bus
local function newBus() return setmetatable({ handlers = {}, prefixes = {}, subs = 0, depth = 0 }, Bus) end

-- ancestors of "a.b.c": "a.b.c", "a.b", "a", "*"  (cached per event name)
function Bus:chain(name)
  local c = self.prefixes[name]
  if c then return c end
  c = { name }
  local pos = #name
  while true do
    local dot
    for i = pos, 1, -1 do
      if sub(name, i, i) == "." then dot = i break end
    end
    if not dot then break end
    insert(c, sub(name, 1, dot - 1))
    pos = dot - 1
  end
  insert(c, "*")
  self.prefixes[name] = c
  return c
end

function Bus:on(name, fn, priority, once)
  local list = self.handlers[name]
  if not list then list = {}; self.handlers[name] = list end
  local h = { fn = fn, priority = priority or 0, once = once, live = true }
  local at = #list + 1
  for i = 1, #list do
    if list[i].priority < h.priority then at = i break end
  end
  insert(list, at, h)
  self.subs = self.subs + 1
  return h
end

function Bus:off(name, h)
  local list = self.handlers[name]
  if not list or not h.live then return end
  for i = 1, #list do
    if list[i] == h then remove(list, i) h.live = false self.subs = self.subs - 1 return end
  end
end

local invocations = 0
function Bus:emit(name, ...)
  local chain = self:chain(name)
  self.depth = self.depth + 1
  for ci = 1, #chain do
    local list = self.handlers[chain[ci]]
    if list then
      -- iterate over a snapshot: handlers may subscribe/unsubscribe while running
      local n = #list
      local snap = {}
      for i = 1, n do snap[i] = list[i] end
      for i = 1, n do
        local h = snap[i]
        if h.live then
          if h.once then self:off(chain[ci], h) end
          invocations = invocations + 1
          h.fn(name, ...)
        end
      end
    end
  end
  self.depth = self.depth - 1
end

-- scenario ---------------------------------------------------------------------------------------------
local bus = newBus()
local total = 0
local counters = {}
local function counter(key)
  return function(name, a, b, c)
    counters[key] = (counters[key] or 0) + 1
    total = (total + (a or 0) + (b or 0) * 2 + (c or 0) * 3) % 1000003
  end
end

local EVENTS = { "net.http.request", "net.http.response", "net.tcp.connect", "ui.click", "ui.key.down", "ui.key.up",
  "db.query.select", "db.query.insert", "timer.tick", "log.warn" }
for i, ev in ipairs(EVENTS) do
  bus:on(ev, counter(ev), i % 3)
  bus:on(ev, counter(ev .. "#2"), 5 - i % 4)
end
bus:on("net", counter("net.any"))
bus:on("ui", counter("ui.any"))
bus:on("db.query", counter("db.query.any"))
bus:on("*", function(name, a) total = (total + #name + (a or 0)) % 1000003 end, -10)
-- a handler that re-emits another event (bounded recursion) and a self-unsubscribing handler
local reh
reh = bus:on("timer.tick", function(name, a, b)
  if a and a % 50 == 0 and bus.depth < 3 then bus:emit("log.warn", a, b) end
end)
local selfOff
selfOff = bus:on("ui.click", function(name, a) if a and a % 4999 == 0 then bus:off("ui.click", selfOff) end end)

local seed = 31
local function rnd(n) seed = (seed * 16807) % 2147483647 return seed % n + 1 end
for i = 1, N do
  local ev = EVENTS[rnd(#EVENTS)]
  local r = rnd(10)
  if r <= 5 then
    bus:emit(ev, i, r)
  elseif r <= 8 then
    bus:emit(ev, i, r, i % 7)
  elseif r == 9 then
    bus:emit(ev)
  else
    -- transient one-shot subscription
    bus:on(ev, counter("once"), 0, true)
    bus:emit(ev, i)
  end
end
local keys = 0
for _ in pairs(counters) do keys = keys + 1 end
print("events", invocations, total, bus.subs, keys)
