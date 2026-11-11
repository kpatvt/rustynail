
local f, g = ...
local function a() local x <close> = f() return 1 end
local function b() local x <close> = f() return g() end
local function c() local x <close> = f(); local y <close> = g(); do local z <close> = nil end end
local function d(...) local x <close> = f() return ... end
local function e() for i = 1, 2 do local x <close> = f(); if i then break end end end
return a, b, c, d, e
