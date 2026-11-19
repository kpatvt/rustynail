-- study micro: constructor function (3 fields + metatable), result escapes; 1M iterations
local V = {}
V.__index = V
local function vec(x, y, z) return setmetatable({x = x, y = y, z = z}, V) end
local keep = {}
for i = 1, 64 do keep[i] = false end
for i = 1, 1000000 do keep[i % 64 + 1] = vec(i, i, i) end
print(0)
