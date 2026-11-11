local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = 131072
local v1 = false - 128.0
local v2 = -0x1p4 | (0.1 >= (-128 == -9223372036854775807))
local v3 = not (1000000007 ^ 256) + -[[long
bracket]]
local v4 = (-65536 >= 9223372036854775808) ~ (f(v0) | t[a]) ~ (~-65536 <= (f(c) > d))
return (-([[long
bracket]] and 65536)) == (256 < c) % true, ~(false >> 0.1)
