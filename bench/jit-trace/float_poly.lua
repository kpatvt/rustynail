-- J5 informational set (b') `float_poly` (docs/JIT_TRACE.md 1.6; the J0 oracle program 2, bench/jit-oracle/float_poly.lua,
-- called repeatedly): two float multiply-add recurrences (latency bound) in a for loop. N = reps (arg[1]); inner n = 1000.
-- Expected stdout (default N): see float_poly.expected
local function f(N)
  local x, acc = 0.5, 0.0
  for i = 1, N do
    acc = acc * 0.999 + x * 1.5 - 0.25
    x = x * 0.75 + 0.1
  end
  return acc
end
local reps = tonumber(arg and arg[1]) or 200000
local total = 0.0
for r = 1, reps do total = total + f(1000) end
print(string.format("%.6f", total))
