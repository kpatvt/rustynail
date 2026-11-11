-- Event emitter (on/off/once/emit with varargs) and a promise-like library with then/catch
-- chaining and all(), driven by a deterministic fake event loop with timers. small: N=3
local N = tonumber(arg and arg[1]) or 4000

local unpack = table.unpack or unpack
local pack = table.pack or function(...) return { n = select('#', ...), ... } end

-- Event loop with virtual time -------------------------------------------------------------
local loop = { now = 0, timers = {}, seq = 0, microtasks = {}, ticks = 0 }

function loop.setTimeout(fn, delay)
  loop.seq = loop.seq + 1
  local t = { at = loop.now + delay, seq = loop.seq, fn = fn }
  local timers = loop.timers
  timers[#timers + 1] = t
  -- keep sorted by (at, seq) using insertion from the tail
  local i = #timers
  while i > 1 and (timers[i - 1].at > t.at or (timers[i - 1].at == t.at and timers[i - 1].seq > t.seq)) do
    timers[i] = timers[i - 1]
    i = i - 1
  end
  timers[i] = t
  return t
end

function loop.queueMicrotask(fn)
  loop.microtasks[#loop.microtasks + 1] = fn
end

local function drainMicrotasks()
  local i = 1
  while i <= #loop.microtasks do
    local fn = loop.microtasks[i]
    loop.microtasks[i] = false
    fn()
    i = i + 1
  end
  loop.microtasks = {}
end

function loop.run()
  drainMicrotasks()
  while #loop.timers > 0 do
    local t = table.remove(loop.timers, 1)
    loop.now = t.at
    loop.ticks = loop.ticks + 1
    t.fn()
    drainMicrotasks()
  end
end

-- Emitter -------------------------------------------------------------------------------------------
local Emitter = {}
Emitter.__index = Emitter

function Emitter.new()
  return setmetatable({ handlers = {} }, Emitter)
end

function Emitter:on(name, fn)
  local list = self.handlers[name]
  if not list then list = {}; self.handlers[name] = list end
  list[#list + 1] = fn
  return fn
end

function Emitter:once(name, fn)
  local wrapper
  wrapper = function(...)
    self:off(name, wrapper)
    return fn(...)
  end
  return self:on(name, wrapper)
end

function Emitter:off(name, fn)
  local list = self.handlers[name]
  if not list then return end
  for i = #list, 1, -1 do
    if list[i] == fn then table.remove(list, i) end
  end
end

function Emitter:emit(name, ...)
  local list = self.handlers[name]
  if not list then return 0 end
  local copy = { unpack(list) }
  for i = 1, #copy do copy[i](...) end
  return #copy
end

-- Promise -----------------------------------------------------------------------------------------
local Promise = {}
Promise.__index = Promise

local function isPromise(v) return type(v) == "table" and getmetatable(v) == Promise end

function Promise.new(executor)
  local p = setmetatable({ state = "pending", callbacks = {} }, Promise)
  local function settle(state, value)
    if p.state ~= "pending" then return end
    p.state, p.value = state, value
    for _, cb in ipairs(p.callbacks) do loop.queueMicrotask(function() cb(state, value) end) end
    p.callbacks = nil
  end
  local function resolve(v)
    if isPromise(v) then
      v:andThen(function(x) settle("fulfilled", x) end, function(e) settle("rejected", e) end)
    else
      settle("fulfilled", v)
    end
  end
  local function reject(e) settle("rejected", e) end
  local ok, err = pcall(executor, resolve, reject)
  if not ok then reject(err) end
  return p
end

function Promise.resolve(v) return Promise.new(function(res) res(v) end) end
function Promise.reject(e) return Promise.new(function(_, rej) rej(e) end) end

function Promise:andThen(onOk, onErr)
  return Promise.new(function(resolve, reject)
    local function handle(state, value)
      local handler
      if state == "fulfilled" then handler = onOk else handler = onErr end
      if not handler then
        if state == "fulfilled" then resolve(value) else reject(value) end
        return
      end
      local ok, result = pcall(handler, value)
      if ok then resolve(result) else reject(result) end
    end
    if self.state == "pending" then
      self.callbacks[#self.callbacks + 1] = handle
    else
      local state, value = self.state, self.value
      loop.queueMicrotask(function() handle(state, value) end)
    end
  end)
end

function Promise:catch(onErr) return self:andThen(nil, onErr) end

function Promise.all(list)
  return Promise.new(function(resolve, reject)
    local results, remaining = {}, #list
    if remaining == 0 then return resolve(results) end
    for i, p in ipairs(list) do
      Promise.resolve(p):andThen(function(v)
        results[i] = v
        remaining = remaining - 1
        if remaining == 0 then resolve(results) end
      end, reject)
    end
  end)
end

local function delay(ms, value, fail)
  return Promise.new(function(resolve, reject)
    loop.setTimeout(function()
      if fail then reject(value) else resolve(value) end
    end, ms)
  end)
end

-- Scenario ---------------------------------------------------------------------------------------------
local seed = 606
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

local log = {}
local function note(fmt, ...) log[#log + 1] = string.format(fmt, ...) end

-- 1. emitter with varargs, once, off
local em = Emitter.new()
local sum, calls = 0, 0
local function adder(...)
  local args = pack(...)
  for i = 1, args.n do sum = sum + args[i] end
  calls = calls + 1
end
em:on("data", adder)
em:once("data", function(a, b) note("once saw %d %d", a, b) end)
local argCount = 0
em:on("data", function(...) argCount = argCount + select('#', ...) end)
for i = 1, N do em:emit("data", i, i * 2, i % 3) end
em:off("data", adder)
local rest = em:emit("data", 1)
note("emitter sum=%d calls=%d args=%d remaining handlers=%d", sum, calls, argCount, rest)

-- 2. promise chains
local chainTotal, failures = 0, 0
for i = 1, N do
  delay(rnd(50), i):andThen(function(v) return v * 2 end)
    :andThen(function(v)
      if v % 7 == 0 then error("bad " .. v, 0) end
      return v + 1
    end)
    :andThen(function(v) return delay(rnd(10), v) end)
    :catch(function(e) failures = failures + 1; return 0 end)
    :andThen(function(v) chainTotal = chainTotal + v end)
end

-- 3. Promise.all over groups with a rejection
local groups = {}
for g = 1, math.max(2, math.floor(N / 20)) do
  local ps = {}
  for k = 1, 5 do ps[k] = delay(rnd(30), g * 10 + k, g % 5 == 0 and k == 3) end
  Promise.all(ps):andThen(function(vals)
    local s = 0
    for _, v in ipairs(vals) do s = s + v end
    groups[#groups + 1] = string.format("g%d=%d", g, s)
  end, function(err)
    groups[#groups + 1] = string.format("g%d!%d", g, err)
  end)
end

-- 4. ordering between microtasks and timers
local order = {}
loop.setTimeout(function() order[#order + 1] = "timer" end, 0)
Promise.resolve(1):andThen(function() order[#order + 1] = "micro1" end)
  :andThen(function() order[#order + 1] = "micro2" end)
order[#order + 1] = "sync"

loop.run()
note("chain total=%d failures=%d", chainTotal, failures)
table.sort(groups)
note("groups: %s", table.concat(groups, " "))
note("order: %s", table.concat(order, ","))
note("virtual time=%d ticks=%d", loop.now, loop.ticks)

for i = 1, #log do print(log[i]) end
