-- E1 (up-recursion unrolling, docs/JIT_TRACE.md section 34): coroutines yielding from inside an unrolled recursion, debug hooks installed
-- from inside it (a count hook, a call hook), debug.traceback / getinfo at depth, a finalizer running between recursions.

-- yield at the leaves of a tree recursion: the frames above are complete when the coroutine is suspended
local function gen(n) if n < 2 then coroutine.yield(n) return n end return gen(n - 1) + gen(n - 2) end
local co = coroutine.wrap(function() for i = 1, 14 do gen(i) end return "done" end)
local ys, nys = 0, 0
while true do local v = co() if v == "done" then break end ys = ys + v nys = nys + 1 end
print("yield", ys, nys)

-- yield in the middle of the recursion (after the first self-call returned)
local function mid(n) if n == 0 then return 0 end local a = mid(n - 1) if n % 4 == 0 then coroutine.yield(a) end return a + n end
local co2 = coroutine.create(function() local s = 0 for i = 1, 40 do s = s + mid(i % 13) end return s end)
local last, steps = nil, 0
repeat local ok, v = coroutine.resume(co2) last = v steps = steps + 1 until coroutine.status(co2) == "dead"
print("yield-mid", last, steps)

-- a count hook installed from inside the recursion (the next poll leaves compiled code)
local function fibh(n) if n < 2 then return n end return fibh(n - 1) + fibh(n - 2) end
for i = 1, 20 do fibh(15) end
local counts = 0
local function arm(n)
  if n == 0 then debug.sethook(function() counts = counts + 1 end, "", 100) return 0 end
  return arm(n - 1) + fibh(12)
end
arm(6)
local r = fibh(18)
debug.sethook()
print("count hook", r, counts > 0)

-- a call hook: the calls of a recursion seen one by one (traces never run with a hook)
local calls = 0
debug.sethook(function(ev) if ev == "call" then calls = calls + 1 end end, "c")
local r2 = fibh(10)
debug.sethook()
print("call hook", r2, calls)

-- traceback from the bottom of an unrolled recursion
local function tb(n) if n == 0 then return debug.traceback("tb", 1) end local s = tb(n - 1) return s end
for i = 1, 30 do tb(i % 7) end
local text = tb(6)
local nframes = 0
for _ in text:gmatch("in upvalue 'tb'") do nframes = nframes + 1 end
for _ in text:gmatch("in local 'tb'") do nframes = nframes + 1 end
for _ in text:gmatch("in function 'tb'") do nframes = nframes + 1 end
print("traceback", nframes)

-- a finalizer that installs a hook, run by a collection between two recursions
local fired = false
do
  local x = setmetatable({}, { __gc = function() fired = true end })
  x = nil
end
collectgarbage()
local r3 = fibh(16)
print("finalizer", fired, r3)

-- recursion inside a coroutine inside a recursion
local function outer(n)
  if n == 0 then
    local c = coroutine.wrap(function() return fibh(11) end)
    return c()
  end
  return outer(n - 1) + 1
end
local o = 0
for i = 1, 40 do o = o + outer(i % 9) end
print("nested", o)
