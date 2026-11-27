-- Section 75.7 (`dynunroll`): counted `for` loops of inlined callees whose bounds are run-time values are unrolled up to a
-- trip-count bound: the trip count changes between calls (0, 1, many, beyond the bound), negative and non-unit steps,
-- a loop entered after it was skipped at record time and the reverse, a nested pair, a loop whose bound comes from a table.
local function sum_to(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function down(a, b)
  local s = 0
  for i = a, b, -1 do s = s * 3 + i end
  return s
end
local function step3(n)
  local t = 0
  for i = 0, n, 3 do t = t + i * i end
  return t
end
local function pairs_(n)
  local c = 0
  for i = 1, n do
    for j = i + 1, n do c = c + i * j end
  end
  return c
end
local function field(t)
  local s = 0
  for i = 1, t.n do s = s + t[i] end
  return s
end

local acc = 0
for k = 1, 300 do
  local n = (k * 7) % 11          -- 0 .. 10
  if k > 200 then n = (k % 3 == 0) and 0 or 25 end -- skipped / beyond the bound late in the run
  acc = acc + sum_to(n) + down(n, n - (k % 5)) + step3(n) + pairs_(n % 6)
end
print("acc", acc)

local t = { n = 0 }
local s2 = 0
for k = 1, 200 do
  t.n = k % 9
  for i = 1, t.n do t[i] = i * k end
  s2 = s2 + field(t)
end
print("field", s2)

-- the loop variable and the count after early exits of the unrolled body
local function find(arr, n, x)
  for i = 1, n do
    if arr[i] == x then return i end
  end
  return -1
end
local arr = {}
for i = 1, 12 do arr[i] = i * i end
local f = 0
for k = 1, 300 do f = f + find(arr, k % 13, (k % 7) * (k % 7)) end
print("find", f)
