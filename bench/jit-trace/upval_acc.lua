-- J5 gate set (b) `upval_acc` (docs/JIT_TRACE.md 1.6). Loop body: acc = acc + t[i] (integers), `acc` an upvalue of the
-- looping function: the upvalue reference hoists and the accumulator stays in a register. N = reps (arg[1]);
-- inner n = 1000.
-- Expected stdout (default N): see upval_acc.expected
local acc = 0
local function run(t, n)
  for i = 1, n do acc = acc + t[i] end
end
local n = 1000
local t = {}
for i = 1, n do t[i] = i % 17 end
local reps = tonumber(arg and arg[1]) or 450000
for r = 1, reps do run(t, n) end
print(acc)
