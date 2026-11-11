
local f, g = ...
local function a() return f() end
local function b() return f(g()) end
local function c() return (f()) end
local function d() return f(), g() end
local function e() local x <close> = nil; return f() end
local function h() for i = 1, 2 do return f(i) end end
return a, b, c, d, e, h
