
local a = ...
local x, y, z = 1000, 1000.0, "s"
local function f() return 1000, 1000.0, "s", 2^53, 2^53 | 0, -0.0, 0.0, 0 end
local function g() return 1000.0, 1000, "s", 2^63, -0.0, 0.0 end
return x, y, z, f, g, 9007199254740992, 9007199254740992.0, 0.0, -0.0, 0
