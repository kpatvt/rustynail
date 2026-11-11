-- errors raised by debug hooks propagate; yielding from a Lua hook fails
local function try(f, ...)
  local ok, err = pcall(f, ...)
  debug.sethook()
  print(ok, err)
end
try(function()
  debug.sethook(function() debug.sethook(); error("boom") end, "l")
  local x = 1
  x = 2
end)
try(function()
  debug.sethook(function() debug.sethook(); error({}, 0) end, "c")
  ;(function() end)()
end)
try(function()
  debug.sethook(function() debug.sethook(); error("in count") end, "", 5)
  for i = 1, 100 do end
end)
local co = coroutine.wrap(function()
  debug.sethook(function() coroutine.yield("y") end, "", 1)
  for i = 1, 3 do end
end)
print(pcall(co))
debug.sethook()
try(function()
  debug.sethook(function(e) if e == "return" then debug.sethook(); error("ret hook") end end, "r")
  return 1
end)
print(debug.gethook())
