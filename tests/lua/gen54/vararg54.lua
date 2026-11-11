
local function f(...) return ... end
local function g(a, b, ...) local x = select('#', ...) return x, a, b, ... end
local function h(...) local a, b = ... return function() return a, b end end
local function k(...) return {...}, {n = select('#', ...), ...} end
return f, g, h, k, ...
