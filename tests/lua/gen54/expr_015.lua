local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = f(a)
local v1 = a ^ (not (t.x | nil))
local v2 = (255.0 << math.mininteger) // f(b) << #(1000000007)
local v3 = ~"tab\there"
local v4 = (127.0 & a) ^ (not (255 or 9223372036854775808))
return (math.maxinteger / t.x) | (a & c) < (t[k] << 7) ^ (-1e100 ~ 7), (v0 / -65536) ~= (0 // v2)
