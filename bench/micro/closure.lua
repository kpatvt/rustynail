local N = 20000000
local a; for i = 1, N // 4 do a = function() return i end; a = function() return i end; a = function() return i end; a = function() return i end end
print(0)
