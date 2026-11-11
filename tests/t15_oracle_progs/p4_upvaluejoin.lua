-- review finding 4: SETUPVAL may alias a slot of the running frame (debug.upvaluejoin).
local z = 0
local f
local function joiner(g)
  for k = 1, 10 do
    local nm = debug.getupvalue(f, k)
    if nm == "z" then debug.upvaluejoin(f, k, g, 1) return end
  end
end
f = function(n, doit)
  local y = 1
  local g = function() return y end
  local s = 0
  for i = 1, n do
    if i == 5 and doit then joiner(g) end
    y = i
    z = 2.5
    s = s + y
  end
  return s
end
print(f(20, false), f(20, true), f(20, false))
