local a, b = 17, 5
print(a + b, a - b, a * b, a // b, a % b, a / b, a ^ 2)
print(-a, -b, a // -b, -a // b, -a // -b, a % -b, -a % b, -a % -b)
print(math.maxinteger + 1 == math.mininteger, math.mininteger - 1 == math.maxinteger)
print(math.maxinteger * 2, math.mininteger * -1)
print(math.mininteger // -1, math.mininteger % -1)
print(7 // 0.0, -7 // 0.0, 0.0 / 0.0 ~= 0.0 / 0.0, 7 % math.huge, -7 % math.huge)
print(5 // 2 * 2 + 5 % 2, 1e15 + 1, 2^53 + 1)
