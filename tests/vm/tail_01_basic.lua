local function loop(n, acc) if n == 0 then return acc end return loop(n - 1, acc + n) end
print(loop(10, 0), loop(100000, 0))
local function a(n) if n == 0 then return "done" end return (function(m) return a(m) end)(n - 1) end
print(a(10000))
local function even(n) if n == 0 then return true end return odd(n - 1) end
function odd(n) if n == 0 then return false end return even(n - 1) end
print(pcall(even, 10))
local function va(n, ...) if n == 0 then return select('#', ...) end return va(n - 1, n, ...) end
print(va(50))
local t = setmetatable({}, {__call = function(self, n) if n == 0 then return "mm" end return self(n - 1) end})
print(t(1000))
