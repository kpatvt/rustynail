-- hooks set or cleared in the middle of a function take effect at once (the interpreter switches
-- between its fast and hooked copies at instruction boundaries)
local lines = {}
local function lhook(_, l) lines[#lines + 1] = l end

-- set by a native call in straight-line code: the next lines are traced
local function f1()
  local a = 1
  debug.sethook(lhook, "l")
  a = a + 1
  a = a * 2
  debug.sethook()
  a = a - 1
  return a
end
print(f1(), table.concat(lines, " "))

-- set from inside a metamethod (the switch waits for the GETFIELD to finish)
lines = {}
local mt = { __index = function(t, k) debug.sethook(lhook, "l"); return k .. "!" end }
local function f2()
  local t = setmetatable({}, mt)
  local v = t.x
  local w = v .. "?"
  debug.sethook()
  return w
end
print(f2(), #lines > 0)

-- count hook armed before a call-free loop: the loop is interrupted by the hook
local n = 0
local function f3()
  debug.sethook(function() n = n + 1; if n == 3 then debug.sethook(); error("stop", 0) end end, "", 1000)
  local i = 0
  while true do i = i + 1 end
end
print(pcall(f3), n)

-- set in a callee, observed by the caller after the return
lines = {}
local function arm() debug.sethook(lhook, "l") end
local function f4()
  local x = 0
  arm()
  x = x + 1
  debug.sethook()
  return x
end
print(f4(), #lines >= 2)

-- set in a comparison metamethod: the conditional jump still completes correctly
lines = {}
local cmp = setmetatable({}, { __lt = function() debug.sethook(lhook, "l"); return true end })
local function f5()
  local r
  if cmp < cmp then r = "yes" else r = "no" end
  debug.sethook()
  return r
end
print(f5(), #lines > 0)

-- a hook on another coroutine is seen when it resumes
local co = coroutine.create(function()
  local s = 0
  for i = 1, 10 do s = s + i; coroutine.yield(s) end
  return s
end)
coroutine.resume(co)
local cnt = 0
debug.sethook(co, function() cnt = cnt + 1 end, "", 1)
coroutine.resume(co)
debug.sethook(co)
print(cnt > 0, coroutine.resume(co))
print(debug.gethook())
