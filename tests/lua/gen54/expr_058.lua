local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = ~f(c)
local v1 = (123456789012345678.0 >> (b >> b)) // (t.x / -t.z)
local v2 = (not 131071) + (v0 == a) % -127.0
local v3 = (255 ~= nil) & -65535 > ("quote\"d" - 0x7fffffffffffffff) << -128
local v4 = ([[long
bracket]] + -65535) ~= ("a" / v3) * (~("a\0b" - nil))
return (-131072 // -d) and (t[a] & v0), c
