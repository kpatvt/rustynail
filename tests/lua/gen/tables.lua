
local a, b = ...
local t1 = {}
local t2 = {1, 2, 3}
local t3 = {x = 1, y = 2, [3] = 4, [a] = b, [1.5] = 2, ["k" .. a] = 1}
local t4 = {1, 2, x = 3, 4, [10] = 5, f = function() end, {1}, {x = {y = {}}}}
local t5 = {a, b, ...}
local t6 = {f(), f(), (f())}
local t7 = {[1] = 1, [2] = 2, n = 2; 3; 4,}
return t1, t2, t3, t4, t5, t6, t7
