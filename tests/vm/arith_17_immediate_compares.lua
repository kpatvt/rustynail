local a, b, c = 5, 5.0, "5"
print(a == 5, b == 5, c == 5, a ~= 5, a < 6, a <= 5, a > 4, a >= 5, a > 5, a < 5)
print(b < 6, b <= 5, b > 4, b >= 5, b > 5, b < 5)
print(a == 500, a < 500, a < -500, a > -500, a >= 127, a <= 128, a < 128)
print(pcall(function() return c < 6 end))
print(pcall(function() return {} < 6 end))
print(pcall(function() return nil >= 6 end))
print(pcall(function() return 6 > {} end))
local nan = 0/0
print(nan < 1, nan > 1, nan <= 1, nan >= 1, nan == 1)
print(a == 5.5, b == 5.5, 5.5 == a, 5 == b, 2^53 == 2^53 | 0)
local m = math.maxinteger
print(m < 127, m > 127, m >= 128, m == 127.0)
