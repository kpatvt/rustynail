-- F4: temporaries live across a loop back-edge and dead after it; while/repeat/goto shapes, short circuits
local function f(n)
  local s, k = 0.0, 0
  while k < n do
    k = k + 1
    s = s + ((k * 1.5 + 0.25) * (k + 2) > 50 and (k * 0.5) or (k * 0.125)) * (k % 3 == 0 and 2.5 or 1.5)
  end
  repeat
    k = k - 1
    s = s - (k * 0.75 + 1) / (k + 1.5)
  until k <= n // 2
  for i = n, 1, -2 do
    s = s + (i * 1.25 - (i // 2) * 0.5) * (i & 3)
    if i % 5 == 0 then goto cont end
    s = s + i * 0.0625 * (i % 7 + 0.5)
    ::cont::
  end
  return s
end
local r = 0.0
for k = 1, 30 do r = r + f(120) end
print(string.format("%.9g", r))
local function g(t)
  local s = 0.0
  for k, v in pairs(t) do s = s + v * (k + 0.5) + (v > 3 and k * 0.25 or v) end
  for i, v in ipairs(t) do s = s + (v * 1.5 - i) * (v + 0.5) end
  return s
end
local t = {}
for i = 1, 50 do t[i] = i * 0.75 end
for k = 1, 20 do g(t) end
print(string.format("%.9g", g(t)))
local function h(n)
  local a, b = 0.0, 1.0
  for i = 1, n do
    a, b = b * 1.0001 + a * 0.5 + (i % 2) * 0.25, a + (b - i * 0.001) * 0.5
  end
  return a, b
end
for k = 1, 20 do h(200) end
print(string.format("%.9g %.9g", h(500)))
