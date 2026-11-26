-- docs/JIT_TRACE.md section 73 (`landfix`): a nest whose inner loop calls a C function and whose inner-loop exit lands on a typed tier-1 label while
-- a pruned dead temporary (the `#r` of the loop body) holds a stale string tag; an open-result call outside the loops (`print(string.format(...))`)
-- makes that temporary dead for `opencall=1`. Same output as the reference in every trace mode and knob setting.
local K = 64
local ring = {}
local rep = string.rep
local T3 = {}
for k = 1, K do T3[k] = (k % 3 == 0) and "ab" or ((k % 3 == 1) and "abc" or "x") end
local R = {}
for k = 1, K do R[k] = 3 + k % 5 end
local acc = 0
for o = 0, 150 do
  for k = 1, K do
    local r = rep(T3[k], R[k], ",")
    ring[k] = r
    acc = acc + #r
  end
  -- (the outer loop's own arithmetic after the inner exit: a mid-block label of typed registers)
  acc = acc + (o % 7) // 2
end
local s = 0
for k = 1, K do s = s + #ring[k] end
print(string.format("%.14g", acc), s)
