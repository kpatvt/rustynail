local N = 20000000
local a, b = 1.0, 1.0000001; for i = 1, N do a = a * b; a = a * b; a = a * b; a = a * b end
print(0)
