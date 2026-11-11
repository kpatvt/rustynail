-- J5 gate set (b) `int_rec2` (docs/JIT_TRACE.md 1.6). Loop body: t[i] = (t[i - 1] + t[i - 2]) & 0xFFFFFF (integers):
-- two forwarded values per iteration. N = reps (arg[1]); inner n = 1000 (pre-filled array part).
-- Expected stdout (default N): see int_rec2.expected
local function run(t, n)
  for i = 3, n do t[i] = (t[i - 1] + t[i - 2]) & 0xFFFFFF end
end
local n = 1000
local t = {}
for i = 1, n do t[i] = 1 end
local reps = tonumber(arg and arg[1]) or 300000
local total = 0
for r = 1, reps do
  t[1] = r
  t[2] = 1
  run(t, n)
  total = total + t[n]
end
print(total, t[n])
