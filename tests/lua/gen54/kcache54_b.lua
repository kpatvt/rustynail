
local t = ...
local function f() return "a", "b", "c", 1.5, 2.5 end
local function g() return "c", "b", "a", 2.5, 1.5, 1.5 end
local function h() local function i() return "b", 1.5, nil, false, true end return i end
return t.a, t.b, t.c, f, g, h, 1.5, "a", nil, true, false
