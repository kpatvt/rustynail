-- M1: mutual recursion of two functions called from a hot loop (an `even` / `odd` pair that also has a generic `for`,
-- so every RETURN carries the close flag `k`).
--   lua m1_mutual.lua   (N iterations)
local isodd
local function iseven(n, tag)
  for _, t in ipairs(tag) do if t == "x" then return false end end
  if n == 0 then return true end
  return isodd(n - 1, tag)
end
function isodd(n, tag)
  for _, t in ipairs(tag) do if t == "x" then return false end end
  if n == 0 then return false end
  return iseven(n - 1, tag)
end
local n = tonumber(arg and arg[1]) or 300000
local tag = { "a" }
local c = 0
for i = 1, n do
  if iseven(i % 9, tag) then c = c + 1 end
  if isodd(i % 5, tag) then c = c + 2 end
end
print(c)
