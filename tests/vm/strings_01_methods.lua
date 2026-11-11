local s = "hello"
print(s:len(), s:rep(2), s:sub(2, 3), s:byte(1), s:byte(1, 3))
print(("x"):rep(3, ","), #s, s:sub(-3), s:sub(2), s:sub(0), s:sub(10), s:sub(-100, 2))
print(string.char(72, 105), ("abc"):byte(-1), ("").len(""), ("abc"):len())
local mt = getmetatable("")
print(type(mt), mt.__index == string, rawequal(mt.__index, string))
print(pcall(function() return ("x"):nomethod() end))
print(pcall(function() local n = 5 return n:foo() end))
print(pcall(function() return #5 end))
