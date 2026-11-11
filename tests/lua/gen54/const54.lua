
local f = ...
local a <const> = 10
local b <const> = "str"
local c <const> = a * 2 + 1
local d <const> = nil
local e <const>, g <const> = 1, f()
local function h() return a, b, c, d, e, g end
return a, b, c, d, e, g, h, a // 0.0, a == 10, b .. b, -c, ~a, not d
