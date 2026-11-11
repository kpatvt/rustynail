local n = 0
for x = 0, 1, 0.25 do n = n + 1 io.write(x, " ") end print(n)
for x = 1, 0, -0.25 do io.write(x, " ") end print()
for x = 1e15, 1e15 + 3 do io.write(x, " ") end print()
n = 0
for x = 0.1, 1.0, 0.1 do n = n + 1 end print(n)
for x = math.huge, math.huge do io.write("inf ") break end print()
for x = -0.0, 0.0 do io.write(x, " ") end print()
for x = 1, 3, 1.0 do io.write(x, " ") end print()
for x = 1.5, 3 do io.write(x, " ") end print()
n = 0
for x = 1, 2, 1e-1 do n = n + 1 end print(n)
