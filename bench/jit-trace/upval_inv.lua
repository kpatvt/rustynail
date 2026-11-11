-- J5 informational set (b') `upval_inv` (docs/JIT_TRACE.md 1.6): invariant upvalue table read in the loop, no SETUPVAL
-- (J4 hoists the upvalue too). Loop body: s = s + w[i]. N = reps (arg[1]); inner n = 1000.
-- Expected stdout (default N): see upval_inv.expected
local w = {}
for i = 1, 1000 do w[i] = (i % 13) * 0.125 end
local function run(n)
  local s = 0.0
  for i = 1, n do s = s + w[i] end
  return s
end
local reps = tonumber(arg and arg[1]) or 700000
local total = 0.0
for r = 1, reps do total = total + run(1000) end
print(string.format("%.6f", total))
