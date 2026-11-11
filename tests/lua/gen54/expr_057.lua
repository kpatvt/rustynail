local a, b, c, d = ...
local t = {x = 1, y = 2, z = 3, 10, 20}
local k = 1
local function f(x) return x end
local v0 = (1e308 <= 256) < ("" // 65535) .. t.x
local v1 = d .. b
local v2 = -(t.y == t[2])
local v3 = ("\x41\65\z  B") >= 256 & f(v1) ^ t[a]
local v4 = (f(d) ^ v0) - #"\u{48}\u{49}" ~ (t.y / 1e15) & 1.0
return math.mininteger, not (c < v2)
