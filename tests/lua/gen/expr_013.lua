local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = (1e308 ~= ~127) * d
local v1 = (not (not t[k])) / [[long
bracket]]
local v2 = [[long
bracket]] ~= (t.x > v1) >> (t.z <= -127)
local v3 = a
local v4 = (0x7fffffff >> "") & (nil ~ 1/0) & (255 and -128) >= ("a\0b" .. -0.0)
return 5e-324, c
