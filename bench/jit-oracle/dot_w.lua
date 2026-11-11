-- Inner-loop shape of matmul (float multiply-add over two arrays) written with `while` loops so that the
-- V2 register allocator (no spilling) fits. Used for the V2 data point only.
local function f(a, b, n, reps)
  local s = 0.0
  local r = 0
  while r < reps do
    r = r + 1
    local i = 1
    while i <= n do
      s = s + a[i] * b[i]
      i = i + 1
    end
  end
  return s
end

local function setup(reps)
  local n = 1000
  local a, b = {}, {}
  for i = 1, n do
    a[i] = (i % 7) / 4.0
    b[i] = (i % 5) / 8.0 + 0.5
  end
  return a, b, n, reps
end

if ORACLE then return f, setup end
print(f(setup(tonumber(arg and arg[1]) or 100000)))
