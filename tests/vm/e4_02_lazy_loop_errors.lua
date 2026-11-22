-- E4 (lazy frames for recursion families with loops, docs/JIT_TRACE.md section 35): stack overflow depth and message, errors and tracebacks
-- from deep inside a recursion whose frames sit in loops, pcall at every level, coroutines yielding inside, debug hooks installed from inside,
-- getinfo / traceback at depth, a finalizer and a collection between recursions.

-- overflow of a recursion inside a loop: the depth reached does not depend on how warm the traces are, the message is the interpreter's
local depths = {}
for r = 1, 4 do
  local d = 0
  local function f(n)
    d = d + 1
    local s = 0
    for i = 1, 1 do s = s + f(n + i) end
    return s
  end
  local ok, e = pcall(f, 1)
  depths[r] = d
  if r == 1 then print("overflow", ok, (tostring(e):gsub("^[^:]*:%d+: ", "")):sub(1, 40)) end
end
print("same depth", depths[1] == depths[2], depths[2] == depths[3], depths[3] == depths[4])

-- recursion close to the limit, entered again and again
local function h(n, lim)
  if n >= lim then return n end
  local r = 0
  for i = 1, 1 do r = h(n + i, lim) end
  return r
end
for _, lim in ipairs { 100, 1000, 20000, 100000 } do
  local ok2, r = pcall(h, 1, lim)
  print("lim", lim, ok2, ok2 and r or "overflow")
end

-- an error at a chosen depth inside the loop: message and the frames of the traceback
local function boom(n, at)
  if n == at then error("boom at " .. n) end
  local s = 0
  for i = 1, 2 do s = s + boom(n + 1, at) end
  return s
end
for _, at in ipairs { 1, 2, 3, 5, 9, 12 } do
  local ok3, msg = xpcall(boom, function(m)
    local tb = debug.traceback(m, 1)
    local frames = 0
    for _ in tb:gmatch("in upvalue 'boom'") do frames = frames + 1 end
    for _ in tb:gmatch("in function 'boom'") do frames = frames + 1 end
    for _ in tb:gmatch("in local 'boom'") do frames = frames + 1 end
    return (tostring(m):gsub("^[^:]*:%d+: ", "")) .. " frames=" .. frames
  end, 1, at)
  print("err", at, ok3, msg)
end

-- pcall at every level, inside the loop, some levels failing
local function pc(n)
  if n == 0 then return 0 end
  local s = 0
  for i = 1, 2 do
    local ok4, r = pcall(pc, n - 1)
    s = s + (ok4 and r or 100)
  end
  if n % 4 == 0 then error("lvl" .. n) end
  return s + 1
end
local acc = 0
for i = 1, 30 do local ok5, r = pcall(pc, i % 9) acc = acc + (ok5 and r or -1) end
print("pcall", acc)

-- an arithmetic error on a value that comes back from deep in the loop recursion
local function bad(n) if n == 0 then return nil end local v for i = 1, 1 do v = bad(n - 1) end return v end
for _, n in ipairs { 0, 1, 2, 5 } do
  local ok6, m = pcall(function() return bad(n) + 1 end)
  print("arith", n, ok6, (tostring(m):gsub("^[^:]*:%d+: ", "")))
end

-- yields at the leaves and in the middle of the loop
local function gen(n)
  if n < 2 then coroutine.yield(n) return n end
  local s = 0
  for i = 1, 2 do s = s + gen(n - i) end
  return s
end
local co = coroutine.wrap(function() for i = 1, 12 do gen(i) end return "done" end)
local ys, nys = 0, 0
while true do local v = co() if v == "done" then break end ys = ys + v nys = nys + 1 end
print("yield", ys, nys)
local function mid(n)
  if n == 0 then return 0 end
  local a = 0
  for i = 1, 2 do a = a + mid(n - 1) if n % 3 == 0 and i == 2 then coroutine.yield(a) end end
  return a + n
end
local co2 = coroutine.create(function() local s = 0 for i = 1, 25 do s = s + mid(i % 8) end return s end)
local last, steps = nil, 0
repeat local _, v = coroutine.resume(co2) last = v steps = steps + 1 until coroutine.status(co2) == "dead"
print("yield-mid", last, steps)

-- a count hook installed from inside the recursion, a call hook
local function fl(n) if n < 2 then return n end local s = 0 for i = 1, 2 do s = s + fl(n - i) end return s end
for i = 1, 20 do fl(14) end
local counts = 0
local function arm(n)
  if n == 0 then debug.sethook(function() counts = counts + 1 end, "", 100) return 0 end
  local s = 0
  for i = 1, 1 do s = s + arm(n - 1) + fl(10) end
  return s
end
arm(5)
local r = fl(16)
debug.sethook()
print("count hook", r, counts > 0)
local calls = 0
debug.sethook(function(ev) if ev == "call" then calls = calls + 1 end end, "c")
local r2 = fl(9)
debug.sethook()
print("call hook", r2, calls)

-- getinfo and traceback from the bottom of a loop recursion
local function gi(n)
  if n == 0 then
    local names = {}
    for l = 1, 5 do local i = debug.getinfo(l, "nl") names[#names + 1] = (i and i.name or "-") .. ":" .. (i and i.currentline or 0) end
    return table.concat(names, " ")
  end
  local r3
  for i = 1, 1 do r3 = gi(n - 1) end
  return r3
end
for i = 1, 30 do gi(i % 6 + 2) end
print("getinfo", gi(4))
local function tb(n) if n == 0 then return debug.traceback("tb", 1) end local s for i = 1, 1 do s = tb(n - 1) end return s end
for i = 1, 30 do tb(i % 7) end
local t = tb(4)
local nf = 0
for _ in t:gmatch("'tb'") do nf = nf + 1 end
print("traceback", nf)

-- a finalizer and full collections between and inside recursions that allocate in their loops
local fin = 0
local function alloc(n)
  if n == 0 then return { n } end
  local t = {}
  for i = 1, 2 do t[i] = alloc(n - 1) end
  if n == 3 then setmetatable({}, { __gc = function() fin = fin + 1 end }) end
  return t
end
for i = 1, 20 do alloc(6) if i % 5 == 0 then collectgarbage() end end
collectgarbage()
collectgarbage()
print("gc", fin > 0)

-- a metatable changed while the method recursion runs (the lookup of the next call sees it)
local A = {}
A.__index = A
local B = setmetatable({}, { __index = A })
B.__index = B
function A:rec(n) if n == 0 then return 1 end local s = 0 for i = 1, 2 do s = s + self:rec(n - 1) end return s end
local o = setmetatable({}, A)
local mr = 0
for i = 1, 30 do mr = mr + o:rec(5) end
function B:rec(n) if n == 0 then return 2 end local s = 0 for i = 1, 2 do s = s + A.rec(self, n - 1) end return s end
setmetatable(o, B)
for i = 1, 5 do mr = mr + o:rec(5) end
print("meta", mr)
