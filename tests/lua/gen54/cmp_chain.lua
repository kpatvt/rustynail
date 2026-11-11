
local a, b, c = ...
local x = a < b == (b < c)
local y = a == b ~= (b == c)
local z = not a == not b
local w = a < 1 and b > 2 or c <= 3 and not (a >= 4)
return x, y, z, w
