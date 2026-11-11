-- A copas/lua-coro style cooperative scheduler: tasks are coroutines that sleep, wait on channels and events;
-- generator iterators via coroutine.wrap in generic for; a producer/consumer pipeline and a timer wheel.
local cocreate, coresume, costatus, coyield, cowrap = coroutine.create, coroutine.resume, coroutine.status, coroutine.yield, coroutine.wrap

local Scheduler = {}
Scheduler.__index = Scheduler
function Scheduler.new()
  return setmetatable({ now = 0, ready = {}, sleeping = {}, nsleep = 0, tasks = 0, switches = 0 }, Scheduler)
end
function Scheduler:spawn(fn, ...)
  local co = cocreate(fn)
  local args = { n = select("#", ...), ... }
  self.ready[#self.ready + 1] = { co = co, args = args }
  self.tasks = self.tasks + 1
  return co
end
function Scheduler:sleep(dt) return coyield("sleep", dt) end
function Scheduler:wait(ev) return coyield("wait", ev) end
function Scheduler:step()
  local ready = self.ready
  self.ready = {}
  for i = 1, #ready do
    local t = ready[i]
    local ok, cmd, arg
    if t.args then
      local a = t.args
      t.args = nil
      ok, cmd, arg = coresume(t.co, self, (table.unpack or unpack)(a, 1, a.n))
    else
      ok, cmd, arg = coresume(t.co, t.value)
    end
    self.switches = self.switches + 1
    if not ok then error(cmd) end
    if costatus(t.co) == "dead" then
      self.tasks = self.tasks - 1
    elseif cmd == "sleep" then
      local wake = self.now + arg
      self.nsleep = self.nsleep + 1
      local s = self.sleeping
      -- insertion sort by wake time (stable)
      local j = #s
      while j >= 1 and s[j].wake > wake do s[j + 1] = s[j]; j = j - 1 end
      s[j + 1] = { wake = wake, task = t }
    elseif cmd == "wait" then
      arg.waiters[#arg.waiters + 1] = t
    else
      self.ready[#self.ready + 1] = t
    end
  end
end
function Scheduler:run()
  while self.tasks > 0 do
    if #self.ready == 0 then
      local s = self.sleeping
      if #s == 0 then break end
      self.now = s[1].wake
      local k = 1
      while s[k] and s[k].wake <= self.now do
        self.ready[#self.ready + 1] = s[k].task
        k = k + 1
      end
      table.move(s, k, #s + k, 1)
    end
    self:step()
  end
end

local Event = {}
Event.__index = Event
function Event.new(sched) return setmetatable({ sched = sched, waiters = {} }, Event) end
function Event:signal(value)
  local w = self.waiters
  self.waiters = {}
  for i = 1, #w do
    w[i].value = value
    self.sched.ready[#self.sched.ready + 1] = w[i]
  end
end

local Channel = {}
Channel.__index = Channel
function Channel.new(sched, cap)
  return setmetatable({ sched = sched, buf = {}, head = 1, tail = 0, cap = cap or 8,
                        not_empty = Event.new(sched), not_full = Event.new(sched) }, Channel)
end
function Channel:send(v)
  while self.tail - self.head + 1 >= self.cap do self.sched:wait(self.not_full) end
  self.tail = self.tail + 1
  self.buf[self.tail] = v
  self.not_empty:signal()
end
function Channel:recv()
  while self.tail < self.head do self.sched:wait(self.not_empty) end
  local v = self.buf[self.head]
  self.buf[self.head] = nil
  self.head = self.head + 1
  self.not_full:signal()
  return v
end

-- generators
local function range_gen(n, step)
  return cowrap(function()
    for i = 1, n, step or 1 do coyield(i) end
  end)
end
local function filter_gen(pred, gen)
  return cowrap(function()
    for v in gen do if pred(v) then coyield(v) end end
  end)
end
local function map_gen(f, gen)
  return cowrap(function()
    for v in gen do coyield(f(v)) end
  end)
end

local N = tonumber(arg and arg[1]) or 500
local total, gsum, last_now, switches = 0, 0, 0, 0
for r = 1, N do
  local sched = Scheduler.new()
  local ch = Channel.new(sched, 16)
  local results = Channel.new(sched, 4)
  -- producers
  for p = 1, 4 do
    sched:spawn(function(s, id)
      for i = 1, 600 do
        ch:send(id * 100000 + i)
        if i % 50 == 0 then s:sleep(id) end
      end
      ch:send(false)
    end, p)
  end
  -- workers
  for w = 1, 3 do
    sched:spawn(function(s)
      local acc = 0
      while true do
        local v = ch:recv()
        if v == false then ch:send(false); break end
        acc = (acc + v % 9973) % 1000000007
        if v % 97 == 0 then s:sleep(1) end
      end
      results:send(acc)
    end)
  end
  -- collector
  sched:spawn(function(s)
    for i = 1, 3 do total = (total + results:recv()) % 1000000007 end
  end)
  -- timers
  for t = 1, 50 do
    sched:spawn(function(s, period)
      for k = 1, 10 do s:sleep(period) end
    end, t % 7 + 1)
  end
  sched:run()
  last_now = last_now + sched.now
  switches = switches + sched.switches
  -- generator pipelines
  for v in map_gen(function(x) return x * x end, filter_gen(function(x) return x % 3 == 0 end, range_gen(3000))) do
    gsum = (gsum + v) % 1000000007
  end
end
print("total", total, "gsum", gsum, "now", last_now, "switches", switches)
