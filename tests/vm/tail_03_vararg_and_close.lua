local function f(...) return select('#', ...) end
local function g(...) return f(...) end
local function h(...) return g(...) end
print(h(), h(nil), h(1, 2, 3))
local function va_tail(n, ...) if n == 0 then return ... end return va_tail(n - 1, ...) end
print(va_tail(1000, "a", "b", "c"))
local log = {}
local function C() return setmetatable({}, {__close = function() log[#log + 1] = "c" end}) end
local function withclose(n) local c <close> = C() if n == 0 then return "x" end return withclose(n - 1) end
print(withclose(5), #log)
local function up(n) local x = n return function() return x end end
local function t2(n) if n == 0 then return up(7) end return t2(n - 1) end
print(t2(100)())
