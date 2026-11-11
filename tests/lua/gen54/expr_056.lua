local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = not 0
local v1 = 128
local v2 = math.maxinteger // (not f(b)) == (t.x >= -9223372036854775807)
local v3 = not 0xffffffffffffffff
local v4 = not (1e15 .. 65535.0) - 0/0
return 127 ^ (t.x), 123456789012345678.0
