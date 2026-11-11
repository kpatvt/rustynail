local mt = {}
mt.__add = function(a, b) return "added" end
mt.__index = function(t, k) return k end
mt.__len = function() return 2.5 end
mt.__eq = function() return 1 end
mt.__lt = function() return nil end
mt.__concat = function(a, b) return 42 end
mt.__call = function(self, x) return x, x end
local o = setmetatable({}, mt)
local o2 = setmetatable({}, mt)
local function f(n)
  local acc, cnt = 0, 0
  for i = 1, n do
    local a = o + i
    local b = o[i]
    local c = #o
    local d = (o == o2)
    local e = o .. "x"
    local v1, v2 = o(i)
    if a == "added" then cnt = cnt + 1 end
    if d then cnt = cnt + 1 end
    acc = acc + b + (c * 2) + e + v1 + v2
  end
  return acc, cnt
end
print(f(100))
do
  local closed = 0
  local function g(n)
    local s = 0
    for i = 1, n do
      local x <close> = setmetatable({}, {__close = function() closed = closed + 1; s = s + 1 end})
      s = s + i
    end
    return s
  end
  print(g(50), closed)
end
