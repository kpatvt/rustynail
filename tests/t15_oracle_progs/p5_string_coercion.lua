local function f(n)
  local s, t = "10", {}
  local acc = 0
  for i = 1, n do
    local a = s + i
    local b = s * 2
    local c = "0x10" + 0
    local d = i // 2
    acc = acc + a + b + c + d
    t[#t + 1] = a .. "x"
  end
  return acc, #t
end
print(f(200))
local function g(n)
  local x = "3"
  local r = 0
  for i = 1, n do
    r = r + -x + (i ~ 5) + (pcall(function() return x | 0 end) and 1 or 0)
    x = i % 2 == 0 and "3" or 3
  end
  return r
end
print(g(100))
