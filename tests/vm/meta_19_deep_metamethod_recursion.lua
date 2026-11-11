local depth = 0
local t
t = setmetatable({}, {__index = function(_, k) depth = depth + 1 if depth < 150 then return t[k] end return depth end})
print(t.x)
depth = 0
local ok, e = pcall(function() local r r = setmetatable({}, {__index = function(_, k) return r[k] end}) return r.x end)
print(ok, e)
local ok2, e2 = pcall(function()
  local u
  u = setmetatable({}, {__add = function(a, b) return a + b end})
  return u + 1
end)
print(ok2, e2)
