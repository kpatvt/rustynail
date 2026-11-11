local function mk(...t)
  return function() return t.n, t[1] end, function(v) t[1] = v end
end
local get, set = mk("a", "b")
print(get())
set("z")
print(get())
local function sum(...t) local s = 0 for i = 1, t.n do s = s + t[i] end return s end
print(sum(), sum(1), sum(1, 2, 3, 4, 5))
local function pass(...t) return select('#', ...), t.n end
print(pass(1, nil, nil))
print(pcall(function(...t) return t.x.y end))
