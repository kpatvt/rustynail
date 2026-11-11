local N = 20000000
local a, b = 1, 2; for i = 1, N do a = b; b = a; a = b; b = a end
print(0)
