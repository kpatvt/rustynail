-- J5 informational set (b') `int_sum` (docs/JIT_TRACE.md 1.6; the J0 oracle program 1, bench/jit-oracle/int_sum.lua,
-- called repeatedly): integer multiply/shift/add/sub chain in a numeric for loop. N = reps (arg[1]); inner n = 1000.
-- Expected stdout (default N): see int_sum.expected
local function f(N)
  local s = 0
  for i = 1, N do s = s + i * 3 - (i >> 1) end
  return s
end
local reps = tonumber(arg and arg[1]) or 400000
local total = 0
for r = 1, reps do total = total + f(1000) end
print(total)
