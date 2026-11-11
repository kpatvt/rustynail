-- J5 informational (task F4): multiple results. swap / divmod (fixed counts), a call with C = 0 feeding a call with B = 0, a
-- RETURN B = 0 through two frames, and math.max fed by a call. N = reps of a 1000-iteration loop.
local function swap(a, b) return b, a end
local function divmod(a, b) return math.floor(a / b), a % b end
local function two(a) return a, a + 3 end
local function sum(a, b, c) return a + b + (c or 0) end
local function wrap(a) return 0, two(a) end
local function deep(a) return wrap(a) end
local max = math.max
local function run(n)
  local s, x, y = 0, 1, 2
  for i = 1, n do
    x, y = swap(x, y)
    local q, r = divmod(i * 7 + x, 5 + y)
    s = s + q + r + sum(two(i)) + sum(deep(i)) + max(two(i))
  end
  return s
end
local reps = tonumber(arg and arg[1]) or 10000
local total = 0
for r = 1, reps do total = total + run(1000) end
print(total)
