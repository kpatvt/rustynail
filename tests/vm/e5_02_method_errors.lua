-- E5 (region-scoped method identity, docs/JIT_TRACE.md section 35.4): stack overflow depth and message, errors and tracebacks (method names)
-- from deep inside a method recursion, pcall at every level, coroutines yielding inside, hooks installed from inside, getinfo at depth.

local C = {}
C.__index = C

-- overflow of a method recursion: same depth cold and warm, the interpreter's message
function C:down(n) self.d = self.d + 1 return self:down(n + 1) + 1 end
local depths = {}
for r = 1, 4 do
  local o = setmetatable({ d = 0 }, C)
  local ok, e = pcall(o.down, o, 1)
  depths[r] = o.d
  if r == 1 then print("overflow", ok, (tostring(e):gsub("^[^:]*:%d+: ", "")):sub(1, 40)) end
end
print("same depth", depths[1] == depths[2], depths[2] == depths[3], depths[3] == depths[4])

-- an error at a chosen depth: message and the method frames of the traceback
function C:boom(n, at)
  if n == at then error("boom at " .. n) end
  local s = 0
  for i = 1, 2 do s = s + self:boom(n + 1, at) end
  return s
end
for _, at in ipairs { 1, 2, 3, 6, 10 } do
  local o = setmetatable({}, C)
  local ok, msg = xpcall(o.boom, function(m)
    local tb = debug.traceback(m, 1)
    local frames = 0
    for _ in tb:gmatch("in method 'boom'") do frames = frames + 1 end
    for _ in tb:gmatch("in function 'C:boom'") do frames = frames + 1 end
    for _ in tb:gmatch("in method 'C:boom'") do frames = frames + 1 end
    return (tostring(m):gsub("^[^:]*:%d+: ", "")) .. " frames=" .. frames
  end, o, 1, at)
  print("err", at, ok, msg)
end

-- pcall at every level, some levels failing
function C:pc(n)
  if n == 0 then return 0 end
  local ok, r = pcall(self.pc, self, n - 1)
  if n % 4 == 0 then error("lvl" .. n) end
  return (ok and r or 100) + 1
end
local acc = 0
for i = 1, 40 do local ok, r = pcall(C.pc, setmetatable({}, C), i % 11) acc = acc + (ok and r or -1) end
print("pcall", acc)

-- an arithmetic error on a field read deep in the recursion
function C:bad(n) if n == 0 then return self.missing + 1 end return self:bad(n - 1) end
for _, n in ipairs { 0, 1, 4 } do
  local ok, m = pcall(C.bad, setmetatable({}, C), n)
  print("arith", n, ok, (tostring(m):gsub("^[^:]*:%d+: ", "")))
end

-- yields at the leaves and in the middle
function C:gen(n)
  if n < 2 then coroutine.yield(n) return n end
  return self:gen(n - 1) + self:gen(n - 2)
end
local co = coroutine.wrap(function() local o = setmetatable({}, C) for i = 1, 12 do o:gen(i) end return "done" end)
local ys, nys = 0, 0
while true do local v = co() if v == "done" then break end ys = ys + v nys = nys + 1 end
print("yield", ys, nys)
function C:mid(n)
  if n == 0 then return 0 end
  local a = self:mid(n - 1)
  if n % 3 == 0 then coroutine.yield(a) end
  return a + n
end
local co2 = coroutine.create(function() local o = setmetatable({}, C) local s = 0 for i = 1, 30 do s = s + o:mid(i % 9) end return s end)
local last, steps = nil, 0
repeat local _, v = coroutine.resume(co2) last = v steps = steps + 1 until coroutine.status(co2) == "dead"
print("yield-mid", last, steps)

-- a count hook installed from inside the recursion, a call hook
function C:fl(n) if n < 2 then return n end return self:fl(n - 1) + self:fl(n - 2) end
local h = setmetatable({}, C)
for i = 1, 20 do h:fl(13) end
local counts = 0
function C:arm(n)
  if n == 0 then debug.sethook(function() counts = counts + 1 end, "", 100) return 0 end
  return self:arm(n - 1) + self:fl(9)
end
h:arm(4)
local r = h:fl(15)
debug.sethook()
print("count hook", r, counts > 0)
local calls = 0
debug.sethook(function(ev) if ev == "call" then calls = calls + 1 end end, "c")
local r2 = h:fl(8)
debug.sethook()
print("call hook", r2, calls)

-- getinfo from the bottom, the receiver seen by debug.getlocal at every level
function C:gi(n)
  if n == 0 then
    local names = {}
    for l = 1, 5 do local i = debug.getinfo(l, "nl") names[#names + 1] = (i and i.name or "-") .. ":" .. (i and i.currentline or 0) end
    local same = 0
    for l = 1, 4 do local _, v = debug.getlocal(l, 1) if v == self then same = same + 1 end end
    return table.concat(names, " ") .. " same=" .. same
  end
  local r3 = self:gi(n - 1)
  return r3
end
local g = setmetatable({}, C)
for i = 1, 30 do g:gi(i % 6 + 2) end
print("getinfo", g:gi(4))

-- debug.setlocal replaces the receiver of a suspended frame from a hook-free leaf (outside the region: a C function runs)
function C:sl(n, tag)
  if n == 0 then
    if tag == "swap" then debug.setlocal(3, 1, setmetatable({ alt = true }, C)) end
    return 0
  end
  local v = self:sl(n - 1, tag)
  return v + (self.alt and 10 or 1)
end
local s = 0
for i = 1, 40 do s = s + setmetatable({}, C):sl(5, (i % 8 == 0) and "swap" or "keep") end
print("setlocal", s)
