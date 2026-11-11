local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = ~##-128
local v1 = ~(-65536 and 1.0) + (not d)
local v2 = (t.x // 0x1p4) <= ("a\0b" - 0) % b
local v3 = b
local v4 = #(t.y - 0xffffffffffffffff) > (not 128)
return #128, v2
