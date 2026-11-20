-- K6: self-recursive local functions whose entry samples are mostly leaves (the entry head must keep sampling until it sees a non-leaf call)
local function mk(n) if n == 0 then return nil end return { v = n, nxt = mk(n - 1) } end
local function shorter(x, y)
  local a, b = x, y
  while b ~= nil do
    if a == nil then return true end
    a, b = a.nxt, b.nxt
  end
  return false
end
local function tail(x, y, z)
  if shorter(y, x) then return tail(tail(x.nxt, y, z), tail(y.nxt, z, x), tail(z.nxt, x, y)) end
  return z
end
local function len(l) local n = 0 while l do n = n + 1 l = l.nxt end return n end
local sum = 0
for i = 1, 300 do sum = sum + len(tail(mk(15), mk(10), mk(6))) end
print("tail", sum)
-- a wide tree: mostly leaves, the fraction of leaves changes with the depth parameter
local function tree(d, w) if d == 0 then return 1 end local s = 0 for i = 1, w do s = s + tree(d - 1, w) end return s end
local acc = 0
for r = 1, 400 do acc = acc + tree(r % 5 + 1, r % 3 + 2) end
print("tree", acc)
-- the leaf test depends on the data, not the depth
local function walk(t) if type(t) ~= "table" then return t end local s = 0 for i = 1, #t do s = s + walk(t[i]) end return s end
local data = { 1, { 2, 3, { 4 } }, 5, { { { 6 }, 7 }, 8 }, 9 }
local w = 0
for i = 1, 600 do w = w + walk(data) end
print("walk", w)
