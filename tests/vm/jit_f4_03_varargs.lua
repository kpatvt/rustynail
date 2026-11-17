-- F4: varargs and multiple results around live temporaries
local function v(...)
  local s = 0.0
  for i = 1, select("#", ...) do
    s = s + (select(i, ...) * 2.5 + i) * (i + 0.5) - (i * 1.25)
  end
  return s, ...
end
local function w(n, ...)
  local acc = 0.0
  for i = 1, n do
    local a, b, c = v(i, i * 2, i * 3.5)
    acc = acc + (a * 0.5 + b) * (c + 1.5) + select("#", v(i, i, i, i))
    local t = { v(i, 2.5) }
    acc = acc + #t * 0.75 + (t[2] or 0) * (i + 0.125)
  end
  return acc, select("#", ...)
end
local r = 0.0
for k = 1, 25 do r = r + (w(40, 1, 2, 3)) end
print(string.format("%.9g", r))
print(w(10, "a", nil, nil))
local function packed(...)
  local t = table.pack(...)
  local s = 0.0
  for i = 1, t.n do s = s + (t[i] or 0.5) * (i + 1.5) end
  return s + t.n * 2.25
end
for k = 1, 20 do packed(1, 2, 3, 4) end
print(packed(1, nil, 3, nil, 5.5), packed(), packed(nil))
print(pcall(function(...) local s = 0 for i = 1, 50 do s = s + (i * 0.5 + select(i % 3 + 1, ...)) end return s end, 1, 2, "x"))
