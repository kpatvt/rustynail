local ev
local function hk(e, l)
  local i = debug.getinfo(2, "nl")
  ev[#ev + 1] = e .. ":" .. tostring(i.name) .. ":" .. tostring(l or i.currentline)
end
local function g(x) return x + 1 end
local function report(tag)
  P(tag, #ev, table.concat(ev, ",", 1, math.min(#ev, 40)))
  P(tag, "tail", table.concat(ev, ",", math.max(1, #ev - 5)))
end
-- 1. hook installed from inside a function called by a compiled loop, removed again later
for _, mask in ipairs{"crl", "c", "r", "l"} do
  ev = {}
  local function loop()
    local s = 0
    for i = 1, 30 do
      if i == 10 then debug.sethook(hk, mask) end
      if i == 15 then debug.sethook() end
      s = s + g(i)
    end
    return s
  end
  P("loop", mask, loop())
  report("m" .. mask)
end
-- 2. count hooks with several periods installed inside the loop (exact instruction counts)
for _, n in ipairs{1, 2, 3, 7, 50} do
  ev = {}
  local cnt = 0
  local function loop()
    local s = 0
    for i = 1, 100 do
      if i == 20 then debug.sethook(function() cnt = cnt + 1 end, "", n) end
      s = s + g(i) * 2
    end
    debug.sethook()
    return s
  end
  P("count", n, loop(), cnt)
end
-- 3. a hook that removes itself after some events, loop continues in compiled code
do
  ev = {}
  local n = 0
  local function self_removing(e, l)
    n = n + 1
    if n == 12 then debug.sethook() end
  end
  local function loop()
    local s = 0
    for i = 1, 200 do
      if i == 5 then debug.sethook(self_removing, "l") end
      s = s + g(i)
    end
    return s
  end
  P("selfremove", loop(), n)
end
-- 4. hook installed from another coroutine: debug.sethook(co, ...)
for _, mask in ipairs{"l", "c", "cr"} do
  ev = {}
  local co = coroutine.create(function(a)
    local s = 0
    for i = 1, 20 do
      s = s + g(i)
      if i == 8 then coroutine.yield("mid") end
    end
    return s
  end)
  P("co first", coroutine.resume(co, 0))   -- runs to i == 8 (compiled), suspended inside a loop
  debug.sethook(co, hk, mask)               -- set while suspended
  P("co second", coroutine.resume(co))      -- continues with the hook (not entered compiled)
  report("co" .. mask)
  P(debug.gethook(co) ~= nil, debug.gethook() == nil)
end
-- 5. hook set on a coroutine before it ever runs, and cleared by the hook of the coroutine itself
do
  ev = {}
  local co = coroutine.wrap(function()
    local s = 0
    for i = 1, 30 do
      s = s + g(i)
      if i == 12 then debug.sethook() end
    end
    return s
  end)
  P("co3", pcall(co))
end
-- 6. main hook inherited by new coroutines
do
  ev = {}
  debug.sethook(hk, "l")
  local co = coroutine.wrap(function()
    local s = 0
    for i = 1, 5 do s = s + g(i) end
    return s
  end)
  local r = co()
  debug.sethook()
  P("inherit", r, #ev)
end
-- 7. hooks installed by finalizers (run from inside the compiled loop's collectgarbage / allocation)
do
  ev = {}
  local fired = 0
  local function make()
    return setmetatable({}, {__gc = function()
      fired = fired + 1
      if fired == 2 then debug.sethook(hk, "l") end
      if fired == 12 then debug.sethook() end
    end})
  end
  local function loop()
    local s = 0
    for i = 1, 40 do
      local o = make()
      o = nil
      if i % 5 == 0 then collectgarbage() end
      s = s + g(i)
    end
    return s
  end
  collectgarbage("stop")
  P("gc loop", loop())
  debug.sethook()
  P("gc fired", fired, #ev > 0)
  -- the same with automatic collections (finalizers run from allocation points inside the loop)
  collectgarbage("restart")
  ev, fired = {}, 0
  local big = 0
  for i = 1, 3000 do
    local o = make()
    local t = {i, i + 1, tostring(i)}
    big = big + #t[3]
  end
  collectgarbage() collectgarbage()
  debug.sethook()
  P("gc auto", big, fired > 10, #ev > 0)
end
-- 8. call hook with ftransfer/ntransfer ('r')
do
  local acc = {}
  local function th(e)
    local i = debug.getinfo(2, "rn")
    acc[#acc + 1] = e .. ":" .. tostring(i.name) .. ":" .. i.ftransfer .. ":" .. i.ntransfer
  end
  local function two(a, b) return a, b, a + b end
  debug.sethook(th, "cr")
  for i = 1, 3 do local x, y, z = two(i, i) end
  debug.sethook()
  P("transfer", table.concat(acc, ","))
end
-- 9. sethook with count + line + gethook values
do
  local h = function() end
  debug.sethook(h, "crl", 100)
  local f, m, c = debug.gethook()
  debug.sethook()
  P("gethook", f == h, m, c)
end
return table.concat(out, "\n")
