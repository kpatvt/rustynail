-- informational (task S0): a tuple returned as a table by an inlined function, consumed by the caller (allocation sinking, 6.8)
local N = tonumber(arg and arg[1]) or 3000000
local function mk(a, b) local t = { x = a, y = b } return t end
local function add(p, q) local r = mk(p.x + q.x, p.y + q.y) return r end
local function f(n)
  local s = 0
  for i = 1, n do
    local a = mk(i, 1)
    local b = mk(2, i)
    local p = add(a, b)
    s = s + p.x - p.y
  end
  return s
end
print(f(N))
