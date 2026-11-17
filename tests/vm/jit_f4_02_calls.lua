-- F4: temporaries live across calls, multiple results, open calls
local function two(x) return x * 2.5, x + 1 end
local function three(x) return x, x * 2, x * 3 end
local function id(...) return ... end
local function f(n)
  local s = 0.0
  for i = 1, n do
    local a, b = two(i)
    s = s + (i * 1.5 + a) * (b - 0.5) + math.sin(i * 0.25) * two(i * 0.5) + select("#", three(i)) + (math.max(i, 3, i * 0.5))
    s = s + (i + 0.5) * select(2, three(i)) - id(i * 2.0) * (i + 1)
    s = s + #{ three(i) } + #{ id(i, i, i), id(i, i) } + (id() or 0.5)
  end
  return s
end
local r = 0.0
for k = 1, 30 do r = r + f(60) end
print(string.format("%.9g", r))
local function g(n)
  local t = {}
  for i = 1, n do
    t[#t + 1] = i * 1.5 + (id(i * 2, 7)) * 0.5
    t[#t + 1] = (select(i % 3 + 1, three(i)))
  end
  local s = 0
  for i = 1, #t do s = s + t[i] * (i + 0.5) end
  return s, #t
end
for k = 1, 20 do g(40) end
print(g(100))
print(pcall(function() local s = 0 for i = 1, 100 do s = s + (i * 1.5 + two(i)) * (nil) end return s end))
