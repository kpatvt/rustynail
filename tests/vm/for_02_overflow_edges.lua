local n = 0
for i = math.maxinteger - 2, math.maxinteger do n = n + 1 io.write(i - math.maxinteger, " ") end print(n)
n = 0
for i = math.mininteger, math.mininteger + 2 do n = n + 1 end print(n)
n = 0
for i = math.mininteger + 2, math.mininteger, -1 do n = n + 1 end print(n)
n = 0
for i = math.maxinteger, math.maxinteger - 2, -1 do n = n + 1 end print(n)
n = 0
for i = 1, math.maxinteger, math.maxinteger // 2 do n = n + 1 io.write(i, " ") end print(n)
n = 0
for i = math.mininteger, math.maxinteger, math.maxinteger do n = n + 1 end print(n)
n = 0
for i = math.maxinteger, math.mininteger, math.mininteger do n = n + 1 end print(n)
for i = 1, math.huge do if i > 3 then break end io.write(i, " ") end print()
for i = 1, -math.huge do io.write("never") end print()
for i = -1, -math.huge, -1 do if i < -3 then break end io.write(i, " ") end print()
for i = 1, 2^63 do if i > 2 then break end io.write(i, " ") end print()
for i = math.maxinteger - 1, 1e100 do io.write("m ") end print()
for i = 3, 0/0 do io.write("never") end print("nan limit")
