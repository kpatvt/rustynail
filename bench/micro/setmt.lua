-- study micro: setmetatable on a fresh table; 1M iterations
local V = {}
V.__index = V
local keep = {}
for i = 1, 64 do keep[i] = false end
for i = 1, 1000000 do keep[i % 64 + 1] = setmetatable({}, V) end
print(0)
