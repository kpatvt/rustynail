local n = tonumber(arg and arg[1]) or 400
local function mk(n, f) local m = {}; for i = 1, n do local r = {}; for j = 1, n do r[j] = f(i, j) end; m[i] = r end; return m end
local a, b = mk(n, function(i, j) return (i + j) / n end), mk(n, function(i, j) return (i * j % 7) / n end)
local c = {}
for i = 1, n do
  local ai, ci = a[i], {}
  for j = 1, n do
    local s = 0.0
    for k = 1, n do s = s + ai[k] * b[k][j] end
    ci[j] = s
  end
  c[i] = ci
end
local tr = 0
for i = 1, n do tr = tr + c[i][i] end
print(string.format("%.6f", tr))
