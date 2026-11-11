print(1 + 2, 2 * 3 + 4, 10 // 3, 10 % 3, 2 ^ 3, 10 / 4, 3 | 4, 7 & 3, 5 ~ 1, ~7, 1 << 4, 256 >> 4)
print(-(-5), - -5, 2^-1, 1/0, -1/0, 0x10 + 0xf, 1e2 + 1, 3 % -2, -3 % 2, 3.5 // 1)
print(5 // 0.0, -5 // 0.0, 0.0 / 0.0 ~= 0.0 / 0.0, 3 % math.huge, -3 % math.huge)
print(1 < 2, 1 == 1.0, "a" .. "b", "a" .. 1 .. 2.0, #"hello", not 1, not nil)
print(math.maxinteger + 1, math.mininteger - 1, 9007199254740992 + 1.0)
local function f() return 1 // 0 end
print(pcall(f))
local function g() return 1 % 0 end
print(pcall(g))
print(0x7fffffffffffffff + 1)
print(pcall(function() return 2^63 | 0 end))
