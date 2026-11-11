local function inner() local x = nil return x.field end
local function middle() return inner() + 1 end
local function outer() local r = middle() return r end
print(pcall(outer))
local function a() error("deep") end
local function b() a() end
local function c() b() end
print(pcall(c))
local function d() local t = setmetatable({}, {__index = function(t, k) error("idx:" .. k) end}) return t.zzz end
print(pcall(d))
local co = function() return pcall(error, "x") end
print(co())
print(pcall(string.rep))
print(pcall(("x").rep, "x", {}))
