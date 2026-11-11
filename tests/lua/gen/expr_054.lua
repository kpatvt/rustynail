local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = ~(t[1]) | (2 and ("a long string constant that goes beyond forty characters in length")) or (2^63 ~= b)
local v1 = v0
local v2 = (-(nil + d)) > "a"
local v3 = (v2 > f(c)) | (f(v2) and 65535) < (not (not c))
local v4 = (v0 and "") / (v1 ^ 'x') or -255.0
return ~t.z, ~(256 .. true)
