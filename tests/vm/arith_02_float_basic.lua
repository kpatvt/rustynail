local x, y = 7.5, 2.0
print(x + y, x - y, x * y, x / y, x // y, x % y, x ^ y)
print(-x, x // -y, -x // y, x % -y, -x % y)
print(1e308 * 10, -1e308 * 10, 5 % math.huge, -5 % math.huge, 5 % -math.huge)
print(0.1 + 0.2, 1/3, 100 / 3, 2^0.5, 10 // 3.0)
print(3 | 0, 2^31 | 0, 3.0 | 0)
print(1e100, 123456789012345678, 1.5e300 * 1.5e300)
