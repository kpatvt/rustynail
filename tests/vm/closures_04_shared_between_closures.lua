local function pair()
  local v = 0
  return function(x) v = x end, function() return v end
end
local set1, get1 = pair()
local set2, get2 = pair()
set1(5) set2(7)
print(get1(), get2())
local x = 1
local function f() x = x + 1 return x end
local function g() return x end
print(f(), g(), f(), g(), x)
x = 100
print(f(), g())
local t = {}
for i = 1, 3 do
  local c = i
  t[i] = {set = function(v) c = v end, get = function() return c end}
end
t[1].set(10) t[3].set(30)
print(t[1].get(), t[2].get(), t[3].get())
