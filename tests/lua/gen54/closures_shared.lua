
local a, b = 1, 2
local function f() a = a + 1; return function() b = b + a; return a, b end end
local g = f()
return g, f, function() return a, b, _ENV end
