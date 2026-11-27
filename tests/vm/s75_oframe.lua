-- Section 75 (`oframe`): snapshots leave out the slots above the top of their innermost frame (registers of inlined
-- callees that have returned). (1) an exit inside an inlined callee whose live registers lie above the caller's top;
-- (2) the permute shape (a returned inlined method frame below a native self-call); (3) collectables in the returned
-- frame, a collection in between, and a later callee whose frame covers the stale slots.
local function g(a, b)
  local x1, x2, x3, x4, x5 = a + 1, a + 2, a + 3, a + 4, a + 5
  if b % 13 == 0 then
    x5 = x5 * 2
  end
  if b % 29 == 0 then
    x3 = x3 - b
  end
  return x1 + x2 + x3 + x4 + x5
end
local function f(n)
  local s = 0
  for i = 1, n do s = s + g(i, i) end
  return s
end
print("inner", f(1000), f(77))

local P = {}
P.__index = P
function P:swap(i, j)
  local tmp = self.v[i + 1]
  self.v[i + 1] = self.v[j + 1]
  self.v[j + 1] = tmp
end
function P:permute(n)
  self.count = self.count + 1
  if n ~= 0 then
    local n1 = n - 1
    self:permute(n1)
    for i = n1, 0, -1 do
      self:swap(n1, i)
      self:permute(n1)
      self:swap(n1, i)
    end
  end
end
for k = 1, 3 do
  local p = setmetatable({ count = 0, v = { 1, 2, 3, 4, 5, 6 } }, P)
  p:permute(5)
  print("permute", k, p.count, table.concat(p.v, ","))
end

local function mk(i)
  local t = { i, tostring(i) }
  local u = { t, i * 2 }
  return #u + t[1]
end
local function cover(a)
  local r1, r2, r3, r4, r5, r6
  r1 = a
  r6 = r1 + 1
  r2, r3, r4, r5 = r6, r6, r6, r6
  return r1 + r2 + r3 + r4 + r5 + r6
end
local function h(n)
  local s = 0
  for i = 1, n do
    s = s + mk(i)
    if i % 50 == 0 then collectgarbage() end
    s = s + cover(i)
  end
  return s
end
print("cover", h(400))
