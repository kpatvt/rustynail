-- J5 informational (task F4): recursion inside a hot loop. fib(8) and ackermann(1, k) are called from a loop in `run`;
-- the recursion is inlined up to the unroll limit and the recorded iteration leaves the trace at the call beyond it
-- (stitching), so this measures the exit-heavy case. N = reps of a 200-iteration loop.
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local function ack(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack(m - 1, 1) end
  return ack(m - 1, ack(m, n - 1))
end
local function run(n)
  local s = 0
  for i = 1, n do s = s + fib(i % 3 + 4) + ack(1, i % 4) end
  return s
end
local reps = tonumber(arg and arg[1]) or 6000
local total = 0
for r = 1, reps do total = total + run(200) end
print(total)
