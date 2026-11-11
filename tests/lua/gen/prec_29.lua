local a, b, c, d = ...
local t = {x = 'x', 'y', 'z'}
local function f(x) return x end
return #t.x + #t[1]
