-- BUNDLE-T: generators, producer / consumer, schedulers, recursive tree walks, coroutine-in-coroutine, pcall inside
local yield, wrap, create, resume, status = coroutine.yield, coroutine.wrap, coroutine.create, coroutine.resume, coroutine.status

-- recursive in-order walk
local function walk(t) if t then walk(t.l) yield(t.v) walk(t.r) end end
local function ins(t, v) if not t then return {v = v} end if v < t.v then t.l = ins(t.l, v) else t.r = ins(t.r, v) end return t end
local tree for _, v in ipairs{5, 3, 8, 1, 4, 7, 9, 2, 6} do tree = ins(tree, v) end
local out = {} for v in wrap(function() walk(tree) end) do out[#out + 1] = v end
print(table.concat(out, " "))

-- permutations
local function permgen(a, n) n = n or #a if n <= 1 then yield(a) else for i = 1, n do a[n], a[i] = a[i], a[n] permgen(a, n - 1) a[n], a[i] = a[i], a[n] end end end
local cnt, last = 0, nil
for p in wrap(function() permgen({1, 2, 3, 4}) end) do cnt = cnt + 1 last = table.concat(p, "") end
print(cnt, last)

-- producer / filter / consumer
local function producer() return create(function() for i = 1, 20 do yield(i) end return nil end) end
local function filter(src) return create(function() while true do local ok, v = resume(src) if not v then return end if v % 3 ~= 0 then yield(v * v) end end end) end
local function consume(src) local s = 0 while true do local ok, v = resume(src) if not v then break end s = s + v end return s end
print(consume(filter(producer())))

-- scheduler with sleeping tasks and a shared queue
local tasks, log = {}, {}
local function spawn(name, n) tasks[#tasks + 1] = {name = name, co = create(function() for i = 1, n do log[#log + 1] = name .. i yield(i) end return name .. "!" end)} end
spawn("a", 3) spawn("b", 1) spawn("c", 4)
while #tasks > 0 do
  for i = #tasks, 1, -1 do
    local t = tasks[i]
    local ok, v = resume(t.co)
    if status(t.co) == "dead" then log[#log + 1] = tostring(v) table.remove(tasks, i) end
  end
end
print(table.concat(log, " "))

-- pcall / xpcall / error inside, yields across them
local pc = wrap(function()
  for i = 1, 3 do
    local ok, e = pcall(function() local x = yield("p" .. i) if x == "err" then error("E" .. i, 0) end return x end)
    yield(tostring(ok) .. ":" .. tostring(e))
  end
  local ok, e = xpcall(function() yield("xp") error("xe", 0) end, function(m) return "H:" .. m end)
  return tostring(ok) .. tostring(e)
end)
print(pc()) print(pc("fine")) print(pc()) print(pc("err")) print(pc()) print(pc(1)) print(pc()) print(pc())

-- yield called directly by pcall (the caller of the yield frame is a C function)
local dp = create(function() return pcall(yield, "inpcall", 2) end)
print(resume(dp)) print(resume(dp, "back", 3)) print(status(dp))
local dp2 = wrap(function() return select("#", pcall(yield)) end)
print(dp2()) print(dp2(nil, nil))

-- a resumed coroutine resuming its parent's sibling (symmetric transfer through a trampoline)
local co1, co2
local function tramp(first) local nxt, v = first, 0 for i = 1, 10 do nxt, v = select(2, resume(nxt, v + 1)) end return v end
co1 = create(function(v) while true do v = yield(co2, v * 2) end end)
co2 = create(function(v) while true do v = yield(co1, v + 3) end end)
print(tramp(co1))

-- iterator protocol: wrap as a stateless-ish iterator with extra state values
local function range(n) return wrap(function() for i = 1, n do yield(i, i * i) end end) end
local s = 0 for i, sq in range(100) do s = s + sq - i end
print(s)

-- many short-lived coroutines
local total = 0
for i = 1, 400 do
  local c = create(function(a, b) local c2 = yield(a + b) return c2 * 2 end)
  local _, x = resume(c, i, 1)
  local _, y = resume(c, x)
  total = total + y
end
print(total)

-- table.unpack / varargs through resume, select('#') with nils
local vv = wrap(function(...) local a = select("#", ...) local b = select("#", yield(a, ...)) return a, b end)
print(vv(nil, nil, nil)) print(vv(nil, nil))
print(select('#', wrap(function(...) return ... end)(1, nil, nil)))
