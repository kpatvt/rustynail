local a, b = ...
print(a, b, select('#', ...))
local function f(...) return ... end
print(f(...))
print((...))
