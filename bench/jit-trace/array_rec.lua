-- J5 gate set (b) `array_rec` (docs/JIT_TRACE.md 1.6). Loop body: t[i] = t[i - 1] * c + x (floats): the load of
-- t[i-1] forwards from the previous iteration's store. N = reps (arg[1]); inner n = 1000 (array part, pre-filled so
-- that every store hits an existing slot).
-- Expected stdout (default N): see array_rec.expected
local function run(t, n, c, x)
  for i = 2, n do t[i] = t[i - 1] * c + x end
end
local n = 1000
local t = {}
for i = 1, n do t[i] = 0.0 end
local reps = tonumber(arg and arg[1]) or 300000
local total = 0.0
for r = 1, reps do
  t[1] = r * 0.25
  run(t, n, 0.999, 0.5)
  total = total + t[n]
end
print(string.format("%.6f %.6f", total, t[n]))
