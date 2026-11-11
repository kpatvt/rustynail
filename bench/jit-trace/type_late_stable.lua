-- J5 gate set (d) `type_late_stable` (docs/JIT_TRACE.md 1.6): the stable twin of type_late. Identical except that the
-- first phase also runs on the float array, so the element type never changes. N = reps (arg[1]) of the second
-- phase; inner n = 1000; 600 calls in the first phase.
-- Expected stdout (default N): see type_late_stable.expected
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
for r = 1, 600 do total = total + sum(tf, n) end
for r = 1, reps do total = total + sum(tf, n) end
print(string.format("%.6f", total))
