-- K2: the shape of awfy/list with a method: a recursive method (found through `self`, not an upvalue) with a data-dependent leaf (most invocations are leaves)
-- and a root-frame tail call; the entry head must see a non-leaf invocation to learn that the function recurses
local function newElement(v) return { val = v, nxt = nil } end
local function makeList(length)
  if length == 0 then return nil end
  local e = newElement(length)
  e.nxt = makeList(length - 1)
  return e
end
local function isShorterThan(x, y)
  local xTail, yTail = x, y
  while yTail ~= nil do
    if xTail == nil then return true end
    xTail = xTail.nxt
    yTail = yTail.nxt
  end
  return false
end
local T = {}
T.__index = T
function T:tail(x, y, z)
  if isShorterThan(y, x) then
    return self:tail(self:tail(x.nxt, y, z), self:tail(y.nxt, z, x), self:tail(z.nxt, x, y))
  else
    return z
  end
end
local obj = setmetatable({}, T)
local sum = 0
for i = 1, 250 do
  local r = obj:tail(makeList(15), makeList(10), makeList(6))
  local n = 0
  while r do n = n + 1 r = r.nxt end
  sum = sum + n
end
print("list", sum)
