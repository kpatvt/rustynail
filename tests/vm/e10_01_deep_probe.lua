-- docs/JIT_TRACE.md section 38 (`deepprobe`): a deep site (a loop whose recording stopped at the recursion limit) records several probe
-- iterations and compiles one of its longest. Shapes: recursions of varying depth called from a hot loop (the rec_loop shape), depths with an
-- even period, a rare deep iteration, a recursion deeper than any limit, a callee switched while the site probes, errors from depth.

-- (1) rec_loop: fib(4..6) and ack(1, 0..3) from one loop
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
local t = 0
for _ = 1, 30 do t = t + run(200) end
print("rec_loop", t)

-- (2) depths with an even period: the deep call every other iteration
local function sum(n) if n == 0 then return 0 end return n + sum(n - 1) end
local s2 = 0
for i = 1, 3000 do s2 = s2 + sum(i % 2 == 0 and 9 or 2) end
print("parity", s2)

-- (3) a rare deep iteration (every 32nd): the tries fall back to whatever comes after the probes
local s3 = 0
for i = 1, 4000 do
  if i % 32 == 0 then s3 = s3 + fib(10) else s3 = s3 + fib(3) end
end
print("rare", s3)

-- (4) deeper than any limit: the deep re-record stops at the limit again (normal penalties, no probes)
local function count(n) if n == 0 then return 0 end return 1 + count(n - 1) end
local s4 = 0
for i = 1, 2000 do s4 = s4 + count(40 + i % 3) end
print("deep", s4)

-- (5) the callee of the probed loop is switched while the site probes and after its trace is installed
local f5
local function a5(n) if n < 2 then return 1 end return a5(n - 1) + a5(n - 2) end
local function b5(n) if n < 2 then return 2 end return b5(n - 1) + b5(n - 2) end
f5 = a5
local s5 = 0
for i = 1, 3000 do
  if i == 300 or i == 2000 then f5 = (f5 == a5) and b5 or a5 end
  s5 = s5 + f5(i % 4 + 3)
end
print("switch", s5)

-- (6) errors from depth inside the probed loop
local function boom(n, k) if n == 0 then if k % 97 == 0 then error("boom " .. k) end return 1 end return 1 + boom(n - 1, k) end
local ok_n, err_n, last = 0, 0, ""
for i = 1, 3000 do
  local ok, v = pcall(boom, i % 5 + 2, i)
  if ok then ok_n = ok_n + v else err_n = err_n + 1; last = tostring(v):match("boom %d+") end
end
print("errors", ok_n, err_n, last)

-- (7) float and table values through the probed recursion
local function tree(n) if n == 0 then return {v = 0.5} end local l, r = tree(n - 1), tree(n - 2 < 0 and 0 or n - 2) return {v = l.v + r.v} end
local s7 = 0
for i = 1, 1500 do s7 = s7 + tree(i % 3 + 2).v end
print("tree", s7)
