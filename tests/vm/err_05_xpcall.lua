local function h(m) return "handled:" .. tostring(m) end
print(xpcall(function() error("e1", 0) end, h))
print(xpcall(function() return 1, 2, 3 end, h))
print(xpcall(function(a, b) return a + b end, h, 3, 4))
print(xpcall(function() local x = nil return x.y end, h))
print(xpcall(error, h, "direct", 0))
print(xpcall(function() error({}) end, function(e) return type(e) end))
local function bad_handler() error("handler fails", 0) end
print(xpcall(function() error("orig", 0) end, bad_handler))
print(pcall(xpcall))
print(pcall(xpcall, print))
local function depth_h(m) local ok, e = pcall(error, "in handler", 0) return m .. "|" .. e end
print(xpcall(function() error("main", 0) end, depth_h))
