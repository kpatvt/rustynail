-- BUNDLE-L (array loops: `aidx` complemented index, `bhoist` hoisted bounds guards, `snapprune` dead-slot pruning): observers inside counted array loops. Every
-- output must equal the reference interpreter's. Loops are warmed up so that traces exist, then something looks at the frame in the middle of a loop:
-- debug.getlocal over all slots (temporaries), traceback, an error through pcall / xpcall with a handler that reads locals, a hook, a coroutine yield.
local function norm(s) return (tostring(s):gsub("0x%x+", "PTR")) end
local N = 60
local a, b = {}, {}
for i = 1, N do a[i] = i * 0.5; b[i] = (i % 7) + 0.25 end

-- 1. a loop that runs past the array: the sum ends at the first nil (error caught), then at an `or` default
local function sum_past(t, lim)
  local s = 0.0
  for k = 1, lim do s = s + (t[k] or 100) * (b[k] or 1) end
  return s
end
for r = 1, 30 do sum_past(a, N) end
print("past", sum_past(a, N), sum_past(a, N + 5), sum_past(a, 3), sum_past({}, 4))

-- 2. getlocal over every slot of the loop's frame, from a function called mid-loop (level 3)
local function locals_of(level)
  local t = {}
  for i = 1, 40 do
    local n, v = debug.getlocal(level, i)
    -- (the reference leaves the error message text in one more temporary than our interpreter: not compared)
    if not n or (n == "(temporary)" and type(v) == "string") then break end
    t[#t + 1] = n .. "=" .. norm(v)
  end
  return table.concat(t, ",")
end
local seen = {}
local function probe(k) if k % 17 == 0 then seen[#seen + 1] = locals_of(3) end end
local function loop_probe(t, lim)
  local s = 0.0
  for k = 1, lim do
    local x = t[k] * b[k]
    s = s + x
    probe(k)
  end
  return s
end
for r = 1, 30 do loop_probe(a, N) end
seen = {}
print("probe", loop_probe(a, N), #seen)
for i = 1, #seen do print(seen[i]) end

-- 3. error in the middle (index k+1 runs past), handler reads the frame
local function err_loop(t, lim)
  local s = 0
  for k = 1, lim do s = s + t[k + 1] end
  return s
end
for r = 1, 30 do pcall(err_loop, a, N - 2) end
local ok, e = pcall(err_loop, a, N + 3)
print("err", ok, norm(e))
print("xp", xpcall(err_loop, function(m) return norm(m) .. "|" .. locals_of(3) end, a, N + 3))
print("tb", xpcall(err_loop, function(m) return (norm(debug.traceback(m, 1)):gsub("\n.*", "")) end, a, N + 3))

-- 4. the array changes size inside the loop
local function grow_loop(t, lim)
  local s = 0
  for k = 1, lim do
    s = s + (t[k] or 0)
    if k == 7 then t[#t + 1] = 1000 end
    if k == 9 then table.remove(t) end
  end
  return s, #t
end
for r = 1, 30 do local t = {} for i = 1, 20 do t[i] = i end grow_loop(t, 20) end
print("grow", grow_loop((function() local t = {} for i = 1, 12 do t[i] = i end return t end)(), 14))

-- 5. __index and __newindex on the array, upvalue alias
local log = {}
local mt = setmetatable({}, { __index = function(_, k) log[#log + 1] = k return -k end, __newindex = function(t, k, v) rawset(t, k, v * 2) end })
for i = 1, 10 do mt[i] = i end
local function mt_loop(lim) local s = 0 for k = 1, lim do s = s + mt[k] end return s end
for r = 1, 30 do mt_loop(10) end
log = {}
print("mt", mt_loop(10), mt_loop(13), #log, log[1], log[#log])
local up = a
local function alias_loop(lim) local s = 0 for k = 1, lim do s = s + up[k]; if k == 5 then up = b end end return s end
for r = 1, 30 do up = a; alias_loop(10) end
up = a
print("alias", alias_loop(10), alias_loop(10))

-- 6. coroutine yields inside the loop
local co = coroutine.wrap(function(t, lim)
  local s = 0.0
  for r = 1, 40 do
    for k = 1, lim do s = s + t[k]; if k % 20 == 0 then coroutine.yield(s) end end
  end
  return "done", s
end)
local ys = {}
local v1, v2 = co(a, N)
while v1 ~= "done" do ys[#ys + 1] = v1; v1, v2 = co() end
print("co", #ys, ys[1], ys[#ys], v2)

-- 7. count hook during the loop (hooks make traces refuse: the loop must still be right)
local hits = 0
debug.sethook(function() hits = hits + 1 end, "", 50)
print("hook", loop_probe(a, N) > 0, hits > 0)
debug.sethook()
