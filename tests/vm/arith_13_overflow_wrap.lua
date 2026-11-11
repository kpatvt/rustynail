local m, M = math.mininteger, math.maxinteger
print(M + 1 == m, m - 1 == M, M * M, m * m, M * 2, m * 2, -m == m)
local x = M
x = x + 1 print(x)
x = x - 1 print(x)
print(M // 2, m // 2, M % 7, m % 7, M * 0, (M + 1) // 1)
for i = 1, 3 do M = M + 1 end
print(M)
print(0x7fffffffffffffff, 0xffffffffffffffff, 0x8000000000000000, 9223372036854775807, 9223372036854775808, -9223372036854775808)
print(tostring(1e15), tostring(1e16), 2^63, -2^63, 1e100, 123456.789e3)
print(255 // 1 | 0, 0xA, 0Xa, 1e2, .5, 5., 3e-2)
