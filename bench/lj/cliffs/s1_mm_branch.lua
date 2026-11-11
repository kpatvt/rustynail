-- S1: branchy metamethod bodies (the branch alternates with the iteration).
--   lua s1_mm_branch.lua   (N iterations)
local V = {}
V.__add = function(a, b)
  if a.x % 2 == 0 then return a.x + b.x else return a.x - b.x end
end
V.__lt = function(a, b) if a.x > b.x then return true end return a.x + 1 == b.x end
local a, b = setmetatable({x = 1}, V), setmetatable({x = 2}, V)
local n = tonumber(arg and arg[1]) or 6000000
local s = 0
for i = 1, n do
  a.x = i
  s = s + (a + b)
  if a < b then s = s + 1 end
end
print(s)
