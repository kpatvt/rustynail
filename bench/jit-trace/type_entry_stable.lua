-- J5 gate set (d) `type_entry_stable` (docs/JIT_TRACE.md 1.6): the stable twin of type_entry. Identical except that the
-- inner accumulator starts as the float `local a = 0.0`, so its type never changes. N = reps (arg[1]); outer n = 20,
-- inner m = 50 (array part, 50 floats).
-- Expected stdout (default N): see type_entry_stable.expected
local function run(t, n, m)
  local out = 0.0
  for i = 1, n do
    local a = 0.0
    for j = 1, m do a = a + t[j] * 0.5 end
    out = out + a
  end
  return out
end
local m = 50
local t = {}
for j = 1, m do t[j] = (j % 9) * 0.25 end
local reps = tonumber(arg and arg[1]) or 500000
local total = 0.0
for r = 1, reps do total = total + run(t, 20, m) end
print(string.format("%.6f", total))
