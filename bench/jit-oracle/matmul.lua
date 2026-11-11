-- Table bucket: 2D arrays as nested tables (matrix multiply); float arithmetic on values loaded from tables.
local function f(a, b, c, n, reps)
  local s = 0.0
  for r = 1, reps do
    for i = 1, n do
      local ai, ci = a[i], c[i]
      for j = 1, n do
        local sum = 0.0
        for k = 1, n do
          sum = sum + ai[k] * b[k][j]
        end
        ci[j] = sum
      end
    end
    s = s + c[n][n]
  end
  return s
end

local function setup(reps)
  local n = 60
  local function mat(seed)
    local m = {}
    for i = 1, n do
      local row = {}
      for j = 1, n do row[j] = ((i * seed + j * 7) % 13) / 4.0 end
      m[i] = row
    end
    return m
  end
  local c = {}
  for i = 1, n do
    local row = {}
    for j = 1, n do row[j] = 0.0 end
    c[i] = row
  end
  return mat(3), mat(5), c, n, reps
end

if ORACLE then return f, setup end
print(f(setup(tonumber(arg and arg[1]) or 300)))
