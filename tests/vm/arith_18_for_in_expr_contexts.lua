local function f(...) return select('#', ...), ... end
print(f(1 + 1, 2 * 3, 7 // 2, 2^2, 7 % 3, 1 << 3, ~0))
local t = {1 + 1, 2 * 3, [1 + 1] = 5, x = 2 // 1}
print(t[1], t[2], t.x)
local s = 0
for i = 1, 3 do s = s + i * i - i // 2 + i % 2 end
print(s)
local a, b, c = 1, 2, 3
a, b, c = a + b, b + c, c + a
print(a, b, c)
a, b = b, a
print(a, b)
local x = (1 + 2) * (3 + 4) - (5 - 6) / (7 + 8) ^ 2
print(x, 2 ^ 3 ^ 2, (2 ^ 3) ^ 2, -2 ^ -2, not 1 == 2, 1 .. 2 == "12")
print(1 + 2 .. 3 + 4, 2 * 3 .. 4, "a" .. 1 + 2)
print(5 > 3 == true, "x" == "x" ~= false)
