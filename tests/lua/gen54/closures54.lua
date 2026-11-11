
local a, b = ...
local function f(x) return function(y) return function(z) return a + b + x + y + z end end end
local t = {}
for i = 1, 3 do t[i] = function() return i, a end end
while a do local c = a; t[#t + 1] = function() c = c + 1 return c end; if b then break end end
repeat local d = b; t[#t + 1] = function() return d end until d
return f, t
