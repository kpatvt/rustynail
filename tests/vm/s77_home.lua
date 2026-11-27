-- Section 77.6 (`sloadhome`): SLOAD values that live in their own stack slot instead of a spill slot. Exits restore other slots from
-- them (moves, swaps), sunk tables take them as fields, links write them back elsewhere, loops carry the slot they were loaded from,
-- floats live in XMM homes, and a towers-shaped recursion links through register-carrying entries.

-- (1) moves and swaps of loaded values with exits on rare paths, many live values (register pressure: values get spilled)
local function swaps(n, a, b, c, d, e, f, g, h)
  local s = 0
  for i = 1, n do
    a, b = b, a
    c, d, e = e, c, d
    if i % 7 == 0 then f, g = g + 1, f end
    if i % 13 == 0 then s = s + h * 2 else s = s + a + b + c + d + e + f + g end
    if i % 29 == 0 then h = h + 0.5 end
  end
  return s, a, b, c, d, e, f, g, h
end
for r = 1, 4 do print("swaps", swaps(400, 1, 2, 3, 4, 5, 6, 7, 1.5)) end

-- (2) loop-invariant loads used across the body, sunk tables built from them, exits that materialise the tables
local function sunk(n, x, y, z, w, scale)
  local acc, last = 0, nil
  for i = 1, n do
    local t = { x = x, y = y, z = z + i, w = w }
    if i % 17 == 0 then last = t end
    acc = acc + (t.x + t.y) * scale + t.z - t.w
  end
  return acc, last and (last.x + last.y + last.z + last.w)
end
for r = 1, 4 do print("sunk", sunk(500, 3, 4, 5, 6, 0.25)) end

-- (3) values moved to other slots at the end of side traces that link back (writebacks), floats and integers mixed
local function moves(n, p, q, fl)
  local u, v, s = p, q, 0.0
  for i = 1, n do
    if i % 3 == 0 then u = q elseif i % 5 == 0 then u = p; v = fl else v = q end
    s = s + u * fl + v
    if i % 11 == 0 then p, q = q, p end
  end
  return s, u, v, p, q
end
for r = 1, 4 do print("moves", moves(600, 2, 9, 1.25)) end

-- (4) a towers-shaped method recursion (function-entry traces, native pushes and returns, register-carrying links)
local T = {}
T.__index = T
function T.new(n)
  local o = setmetatable({ piles = { {}, {}, {} }, moves = 0 }, T)
  for i = n, 1, -1 do o.piles[1][#o.piles[1] + 1] = i end
  return o
end
function T:push(p, d)
  local pile = self.piles[p]
  local top = pile[#pile]
  if top and d >= top then error("bad move") end
  pile[#pile + 1] = d
end
function T:pop(p)
  local pile = self.piles[p]
  local d = pile[#pile]
  pile[#pile] = nil
  return d
end
function T:move1(from, to)
  self:push(to, self:pop(from))
  self.moves = self.moves + 1
end
function T:move(n, from, to)
  if n == 1 then
    self:move1(from, to)
  else
    local other = 6 - from - to
    self:move(n - 1, from, other)
    self:move1(from, to)
    self:move(n - 1, other, to)
  end
end
for r = 1, 6 do
  local t = T.new(9)
  t:move(9, 1, 3)
  print("towers", r, t.moves, #t.piles[3])
end
