-- an accumulator turns from integer to float in the middle of a long loop, and an array element changes type
local function f(t, n, flip)
  local s = 0
  for i = 1, n do
    if i == flip then s = s + 0.5 end
    s = s + t[(i % #t) + 1]
  end
  return s
end
local a = {}
for i = 1, 50 do a[i] = i end
for rep = 1, 8 do print(f(a, 1200, 300 * rep % 1100 + 1)) end
a[25] = 2.5
for rep = 1, 4 do print(f(a, 1200, 700)) end
