local x = 5
print(x << 1, x << 2, x >> 1, x << 63, x << 64, x >> 64, 1 << x, -1 >> x, 8 >> x)
print(x << 0, x >> 0, 3 << x, 3 >> x)
local f = 5.0
print(f << 1, f >> 1, 1 << f, 1 >> f)
print(pcall(function() local q = 5.5 return q << 1 end))
print(pcall(function() local q = 5.5 return 1 << q end))
