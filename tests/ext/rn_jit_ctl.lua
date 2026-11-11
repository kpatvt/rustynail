-- The `jit` control table: shape, on/off/flush/status semantics, and safety of switching and flushing while compiled
-- code and traces are live (hot loops, coroutines, metamethods). The output is the same with the JIT on or off
-- (RUSTYNAIL_JIT=off): scenarios print checksums and consistency flags, never the mode itself.
print("table", type(jit), type(jit.version), type(jit.version_num), jit.os, jit.arch)
print("version", jit.version:match("^rustynail %d+%.%d+%.%d+ JIT$") ~= nil, jit.version_num >= 0)
print("loaded", package.loaded.jit == jit, require("jit") == jit)
for _, f in ipairs { "on", "off", "flush", "status" } do io.write(f, "=", type(jit[f]), " ") end
print()

local initial = jit.status()
print("status type", type(initial), select("#", jit.status()) >= 1)
local feats = { select(2, jit.status()) }
local f_ok = true
for _, f in ipairs(feats) do if type(f) ~= "string" then f_ok = false end end
print("features strings", f_ok, initial == (#feats > 0))

-- on/off toggling is observable through status() (when a compiler exists), and a no-op otherwise
jit.off()
local off = jit.status()
jit.on()
local on = jit.status()
print("toggle", off == false, on == initial)
jit.off(print) jit.on(print) jit.off(true) jit.on(true) jit.off(print, true) jit.on(print, true) jit.flush()
print("per-function forms accepted", jit.status() == initial)

-- a workload with loops, calls, table access and string work (compiled code and traces if the JIT runs)
local function work(n)
  local t, acc = {}, 0
  for i = 1, n do
    t[i % 64 + 1] = (t[i % 64 + 1] or 0) + i
    acc = acc + t[i % 64 + 1] % 7
    if i % 5 == 0 then acc = acc + #tostring(i) end
  end
  return acc
end
local expect = work(20000)

-- jit.flush() in a hot loop
local acc, t = 0, {}
for i = 1, 200000 do
  t[i % 64 + 1] = (t[i % 64 + 1] or 0) + i
  acc = acc + t[i % 64 + 1] % 7
  if i % 5 == 0 then acc = acc + #tostring(i) end
  if i % 20000 == 0 then jit.flush() end
end
local ref, tt = 0, {}
for i = 1, 200000 do
  tt[i % 64 + 1] = (tt[i % 64 + 1] or 0) + i
  ref = ref + tt[i % 64 + 1] % 7
  if i % 5 == 0 then ref = ref + #tostring(i) end
end
print("flush in loop", acc == ref)

-- jit.off() / jit.on() inside a hot loop and in a nested function call
local function toggler(i) if i % 7000 == 0 then jit.off() elseif i % 7000 == 3500 then jit.on() end end
local s = 0
for i = 1, 100000 do s = s + (i * 3) % 11; toggler(i) end
local s2 = 0
for i = 1, 100000 do s2 = s2 + (i * 3) % 11 end
jit.on()
print("toggle in loop", s == s2)

-- flush while compiled frames are on the stack (recursion), from deep inside
local function rec(n, flush_at)
  if n == 0 then return 0 end
  if n == flush_at then jit.flush() end
  return n % 3 + rec(n - 1, flush_at)
end
local total = 0
for r = 1, 200 do total = total + rec(150, 75) end
print("flush in recursion", total == 200 * (function() local x = 0 for n = 1, 150 do x = x + n % 3 end return x end)())

-- flush / off from a metamethod called by compiled code
local mm = setmetatable({}, { __index = function(_, k) if k % 1000 == 0 then jit.flush() end return k % 5 end,
                              __add = function(a, b) if b % 2500 == 0 then jit.off() jit.on() end return b end })
local ms = 0
for i = 1, 60000 do ms = ms + mm[i] + (mm + i) % 3 end
local mr = 0
for i = 1, 60000 do mr = mr + i % 5 + i % 3 end
print("metamethod", ms == mr)

-- from coroutines (also yielding right after a flush)
local co = coroutine.wrap(function()
  local c = 0
  for i = 1, 90000 do
    c = c + i % 9
    if i % 9000 == 0 then jit.flush(); coroutine.yield(c) end
    if i % 13000 == 0 then jit.off() end
    if i % 13000 == 6500 then jit.on() end
  end
  return c
end)
local last
for i = 1, 11 do last = co() end
local cr = 0
for i = 1, 90000 do cr = cr + i % 9 end
jit.on()
print("coroutine", last == cr)
-- several coroutines running the same hot function while one flushes
local function hot(n) local x = 0 for i = 1, n do x = x + i % 13 end return x end
local cos = {}
for k = 1, 4 do
  cos[k] = coroutine.wrap(function() local r = 0 for round = 1, 20 do r = r + hot(5000) if k == 2 then jit.flush() end coroutine.yield() end return r end)
end
for round = 1, 20 do for k = 1, 4 do cos[k]() end end
local fin = {}
for k = 1, 4 do fin[k] = cos[k]() end
print("coroutines", fin[1] == fin[2] and fin[2] == fin[3] and fin[3] == fin[4] and fin[1] == 20 * hot(5000))
print("final", work(20000) == expect, jit.status() == initial)
