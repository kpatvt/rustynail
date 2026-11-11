local callable = setmetatable({}, {__call = function(self, a, b) return "called", a, b end})
print(callable(1, 2))
print(pcall(callable, 3))
local c2 = setmetatable({}, {__call = callable})
print(c2("x"))
local c3 = setmetatable({}, {__call = function(self, ...) return select('#', ...), ... end})
print(c3(), c3(nil), c3(1, 2, 3))
local cnt = 0
local c4 = setmetatable({}, {__call = function(self) cnt = cnt + 1 return cnt end})
print(c4(), c4(), c4())
print(pcall(setmetatable({}, {__call = 42})))
print(pcall(function() local x = {} x() end))
print(pcall(function() local x = 5 x() end))
local t = {f = setmetatable({}, {__call = function(_, a) return a * 2 end})}
print(t.f(21), t.f(5))
local function tail() return callable(7, 8) end
print(tail())
