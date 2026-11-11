local smt = getmetatable("")
local mt = {__add = function(a, b) return "added" end}
local t = setmetatable({}, mt)
print("10" + 5, pcall(function() return "x" + 1 end))
print(pcall(function() return "x" + t end))
print(pcall(function() return t + "x" end))
print(pcall(function() return "1" + t end))
smt.__index = function(s, k) return k .. "?" end
print(("x").foo, ("x").bar)
smt.__index = string
print(("abc"):len())
smt.__add = nil
print(pcall(function() return "10" + 5 end))
