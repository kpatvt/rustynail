-- J5 gate set (d) `type_late` (docs/JIT_TRACE.md 1.6): the looping function is first called on an integer array, which
-- makes its loop hot and recorded with integer element loads, and afterwards only on float arrays (the recorded
-- element-type guard then fails at every call). Stable twin: type_late_stable (float arrays throughout, same
-- number of calls). N = reps (arg[1]) of the float phase; inner n = 1000; 600 integer-phase calls.
-- Expected stdout (default N): see type_late.expected
local function sum(t, n)
  local s = 0.0
  for i = 1, n do s = s + t[i] * 0.5 end
  return s
end
local n = 1000
local ti, tf = {}, {}
for i = 1, n do ti[i] = i % 7; tf[i] = (i % 7) + 0.5 end
local reps = tonumber(arg and arg[1]) or 500000
local total = 0.0
for r = 1, 600 do total = total + sum(ti, n) end
for r = 1, reps do total = total + sum(tf, n) end
print(string.format("%.6f", total))
