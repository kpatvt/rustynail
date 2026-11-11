local function mk()
  local x = 1
  local function inc() x = x + 1 return x end
  return inc, function() return x end
end
local inc, get = mk()
inc() inc()
print(get())
local function mk2()
  local t = {}
  for i = 1, 3 do
    local v = i * 2
    t[i] = function() v = v + 1 return v end
  end
  return t
end
local t = mk2()
print(t[1](), t[1](), t[2](), t[3]())
local function many()
  local a, b, c = 1, 2, 3
  return function() return a end, function() b = b + 1 return b end, function() return c end
end
local f1, f2, f3 = many()
print(f1(), f2(), f2(), f3())
