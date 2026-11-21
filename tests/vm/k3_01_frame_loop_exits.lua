-- K3: loop exits inside inlined frames. `isShorterThan` is a loop of a callee that the root trace of `tail` unrolls; the exits of the unrolled copy
-- (the end of one list, then `return`) have short side paths: they must still get side traces (a retry of an exit that stayed hot is not held to
-- `framemin`; a side recording that reaches the callee's loop head goes on instead of aborting `SideShort`). The second part is the same shape with
-- a `while` loop and several early returns of different types, called from a numeric `for`.
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
local function tail(x, y, z)
  if isShorterThan(y, x) then
    return tail(tail(x.nxt, y, z), tail(y.nxt, z, x), tail(z.nxt, x, y))
  else
    return z
  end
end
local function length(l)
  local n = 0
  while l do n = n + 1 l = l.nxt end
  return n
end
local sum = 0
for i = 1, 120 do
  sum = sum + length(tail(makeList(15), makeList(10), makeList(6)))
end
print("list", sum)

local function find(t, k)
  local i = 1
  while t[i] ~= nil do
    if t[i] == k then return i end
    i = i + 1
  end
  return false
end
local data = {}
for i = 1, 12 do data[i] = i * 3 end
local function scan(n)
  local acc = 0
  for i = 1, n do
    local r = find(data, (i % 14) * 3)
    if r then acc = acc + r else acc = acc - 1 end
  end
  return acc
end
local total = 0
for r = 1, 40 do total = total + scan(60) end
print("scan", total)
