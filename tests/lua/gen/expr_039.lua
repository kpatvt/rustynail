local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = c
local v1 = "a"
local v2 = true or ("" | v0) and (0.1 - false)
local v3 = (false - t.y) ^ (v2 == t[1]) % (nil ~= a)
local v4 = v2
return 65535, (t.y == 2^63) // (0.1)
