local function rec(n) return 1 + rec(n + 1) end
local ok, e = pcall(rec, 1)
print(ok, e)
local depth = 0
local function rec2() depth = depth + 1 rec2() depth = depth - 1 end
print(pcall(rec2))
print(depth > 1000, depth < 300000)
local t = setmetatable({}, {__index = function(t, k) return t[k + 1] end})
local ok4, e4 = pcall(function() return t[1] end)
print(ok4, e4)
print("still alive")
