-- List: Takeuchi-like recursion over linked lists (port from the public are-we-fast-yet "List" description):
-- result = tail(makeList(15), makeList(10), makeList(6)); verification: its length is 10.
-- Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
local N = tonumber(arg and arg[1]) or 350

local Element = {}
Element.__index = Element
local function newElement(v) return setmetatable({ val = v, nxt = nil }, Element) end
function Element:length()
  if self.nxt == nil then return 1 end
  return 1 + self.nxt:length()
end

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

local function tail(x, y, z)
  if isShorterThan(y, x) then
    return tail(tail(x.nxt, y, z), tail(y.nxt, z, x), tail(z.nxt, x, y))
  else
    return z
  end
end

local sum = 0
for i = 1, N do
  local r = tail(makeList(15), makeList(10), makeList(6))
  sum = sum + r:length()
end
print("list", sum)
