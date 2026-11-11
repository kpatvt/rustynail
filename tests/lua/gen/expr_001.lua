local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = (#-65536 ^ (1e100 ^ 131072)) % (-(t[2] ~ 127))
local v1 = 1e-320
local v2 = f(d) % (v0 .. c)
local v3 = ~0x1p4
local v4 = b
return (not 1/0) == (2^63 == 1) + 3.14159, (f(c) > c) > (d ~ v4)
