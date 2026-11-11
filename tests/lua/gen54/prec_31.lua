local a, b, c, d = ...
local t = {x = 'x', 'y', 'z'}
local function f(x) return x end
return f(a) .. f(b) .. f(c)
