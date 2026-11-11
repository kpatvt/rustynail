local function counter()
  local n = 0
  return function() n = n + 1; return n end
end
local c = counter()
c(); c()
local function va(...)
  local a, b = ...
  return select('#', ...), a, b
end
local x <const> = 42
do
  local y <close> = setmetatable({}, {__close = function() end})
  print(x, y, va(1, 2, 3))
end
local big = "an identical string, long enough to be a long string constant in the constant table"
local big2 = "an identical string, long enough to be a long string constant in the constant table"
print(big == big2, math.mininteger, math.maxinteger, -1, 1 << 62)
