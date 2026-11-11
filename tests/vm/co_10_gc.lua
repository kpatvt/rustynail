local weak = setmetatable({}, {__mode = "v"})
do
  local co = coroutine.create(function() local big = {} for i = 1, 100 do big[i] = {i} end coroutine.yield() end)
  weak[1] = co
  coroutine.resume(co)
end
collectgarbage() collectgarbage()
print(weak[1] == nil)
local live = coroutine.wrap(function() local t = {} for i = 1, 50 do t[i] = tostring(i) coroutine.yield(#t) collectgarbage() end return table.concat(t, ",") end)
local last for i = 1, 50 do last = live() end
print(last) print(#live())
local fin = {}
do
  local c = coroutine.create(function() local x <close> = setmetatable({}, {__gc = function() fin[#fin + 1] = "gc" end}) coroutine.yield() end)
  coroutine.resume(c)
end
collectgarbage() collectgarbage()
print(#fin)
