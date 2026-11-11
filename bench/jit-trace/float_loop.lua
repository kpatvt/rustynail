-- J5 informational set (b') `float_loop` (docs/JIT_TRACE.md 1.6; the J0 oracle program 7, bench/jit-oracle/float_loop.lua,
-- called repeatedly): a float for loop. N = reps (arg[1]); inner 1000 iterations (x = 0.5 .. 500.0 step 0.5).
-- Expected stdout (default N): see float_loop.expected
local function f()
  local s = 0.0
  for x = 0.5, 500.0, 0.5 do s = s + x * 0.25 end
  return s
end
local reps = tonumber(arg and arg[1]) or 700000
local total = 0.0
for r = 1, reps do total = total + f() end
print(string.format("%.6f", total))
