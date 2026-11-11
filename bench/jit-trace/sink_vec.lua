-- informational (task S0): temporary {x=, y=} vectors that never escape the loop body (allocation sinking, 6.8)
local N = tonumber(arg and arg[1]) or 3000000
local function f(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = i + 1 }
    local w = { x = v.y, y = v.x + 2 }
    s = s + v.x * w.y - w.x
  end
  return s
end
print(f(N))
