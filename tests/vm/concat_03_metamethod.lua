local mt = {__concat = function(a, b)
  local x = type(a) == "table" and a.name or tostring(a)
  local y = type(b) == "table" and b.name or tostring(b)
  return "[" .. x .. "+" .. y .. "]"
end}
local A = setmetatable({name = "A"}, mt)
local B = setmetatable({name = "B"}, mt)
print(A .. "s", "s" .. A, A .. B, 1 .. A, A .. 1)
print("a" .. "b" .. A, A .. "a" .. "b", "a" .. A .. "b")
print("x" .. A .. B .. "y")
print(1 .. 2 .. A)
local mt2 = {__concat = function(a, b) return 42 end}
local C = setmetatable({}, mt2)
print(C .. "x", "x" .. C, "a" .. "b" .. C .. "c")
print(pcall(function() return A .. nil end))
