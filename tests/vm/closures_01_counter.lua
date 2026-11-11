local function counter()
  local n = 0
  return function() n = n + 1 return n end, function() return n end, function(v) n = v end
end
local inc, get, set = counter()
inc() inc()
print(get(), inc(), get())
set(100)
print(get(), inc())
local inc2, get2 = counter()
print(inc2(), get2(), get())
