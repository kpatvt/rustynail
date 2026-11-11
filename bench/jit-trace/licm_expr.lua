-- J5 gate set (b) `licm_expr` (docs/JIT_TRACE.md 1.6). Loop body: s = s + t[i] * (sqrt(a*a + b*b) / c), where `sqrt`
-- is an upvalue holding math.sqrt and a, b, c are invariant float locals: peeling hoists the invariant
-- fmul/fadd/FSQRT/fdiv and the callee-identity guard. N = reps (arg[1]); inner n = 1000 (array part, 1000 elements).
-- Expected stdout (default N): see licm_expr.expected
local sqrt = math.sqrt
local function run(t, n, a, b, c)
  local s = 0.0
  for i = 1, n do s = s + t[i] * (sqrt(a * a + b * b) / c) end
  return s
end
local n = 1000
local t = {}
for i = 1, n do t[i] = (i % 10) * 0.5 end
local reps = tonumber(arg and arg[1]) or 150000
local total = 0.0
for r = 1, reps do total = total + run(t, n, 3.0, 4.0, 5.0) end
print(string.format("%.6f", total))
