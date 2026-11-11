local N = 20000000
local t = {x = 1, y = 2}; for i = 1, N do t.x = i; t.y = i; t.x = i; t.y = i end
print(0)
