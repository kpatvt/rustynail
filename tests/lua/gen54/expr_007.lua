local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = (b ^ 0x7fffffff) and 9223372036854775807
local v1 = (not (f(b) / 9223372036854775808)) <= (t.y ~= (b / false))
local v2 = (not (65535 + "tab\there")) == c
local v3 = (1e100 < true) / t.z ^ (#(256 ^ 131072))
local v4 = d
return v4 // (f(v2) * math.maxinteger) .. 100.0, a >> nil
