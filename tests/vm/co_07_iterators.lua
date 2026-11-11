local function it() return coroutine.wrap(function() for i = 1, 3 do coroutine.yield(i) end end) end
local co = coroutine.wrap(function()
  for i in it() do coroutine.yield("outer" .. i) end
  for k, v in pairs(setmetatable({}, {__pairs = function(t) return function(_, k) if not k then coroutine.yield("pairs") return 1, 1 end end, t, nil end})) do print(k, v) end
  for i in function(_, c) c = (c or 0) + 1 if c <= 2 then coroutine.yield("gen" .. c) return c end end do end
  return "end"
end)
local v repeat v = co() print(v) until v == "end"
