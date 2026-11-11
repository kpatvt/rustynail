local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = b + (127 << (t[2] or ""))
local v1 = (0x7fffffff > f(b)) and t.x + (c | f(d)) / ("a\0b" | a)
local v2 = ~v1 and -1
local v3 = (1 >= "\n") <= 5e-324 // ("tab\there" % t[1]) and (t.z <= 127)
local v4 = ~(-127) ^ ~(0x.8)
return not (65536 < v2) .. (not a), (-1 // 0.0)
