-- study micro: Lua->Lua tail call; 1M iterations
local function g(a, b) return a + b end
local function f(a, b) return g(a, b) end
local s = 0
for i = 1, 1000000 do s = s + f(i, 1) end
print(s)
