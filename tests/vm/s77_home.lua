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

-- (5) a loop invariant with a home whose slot number must stay reserved after the scan crosses `LOOP` (the `pairs` loops below keep the
-- table and the iterator invariant; other pre-roll values are spilled): bh_next's `over` with the main chunk's locals before it
local arr5, hash5, mixed5, empty5 = {}, {}, {}, {}
for i = 1, 40 do arr5[i] = i end
local function over(n)
  local total = 0
  for i = 1, n do
    local t = {}
    for j = 1, 10 do t[j] = j t["s" .. j] = j end
    for k, v in pairs(t) do t[k] = v * 2 end
    for k, v in pairs(t) do total = total + v end
  end
  return total
end
print("over", over(300), #arr5, #hash5 + #mixed5 + #empty5)

-- (6) a side trace that writes back the slot of a loaded value and links through a register-carrying entry, whose moves run after the
-- writeback (stores through `__newindex` function / table metatables, dm_dynkey's case E): the slot's home would read the new value
do
local out6 = {}
local function emit(...)
  local t = { ... }
  for i = 1, select("#", ...) do t[i] = tostring(t[i]) end
  out6[#out6 + 1] = table.concat(t, ",")
end
local function loader(src, ...) return assert(load(src))(...) end
local names = {}
for i = 1, 300 do names[i] = "k" .. i end
-- E: stores through __newindex (function and table), present / absent / nil-valued keys
local logs = 0
local sink = {}
local nt = setmetatable({}, { __newindex = function(t, k, v) logs = logs + 1; rawset(t, k, v) end })
local nt2 = setmetatable({}, { __newindex = sink })
local STORE = [==[
local names = ...
return function(t, n, m)
  for i = 1, n do t[names[i % m + 1]] = i end
end]==]
local store = loader(STORE, names)
for r = 1, 6 do
  store(nt, 70, 10 + r * 3)
  store(nt2, 70, 10 + r * 3)
  if r == 3 then for i = 1, 40 do nt[names[i]] = nil end end
  local s1, s2 = 0, 0
  for i = 1, 60 do s1 = s1 + (rawget(nt, names[i]) or 0); s2 = s2 + (sink[names[i]] or 0) + (rawget(nt2, names[i]) or 0) end
  emit("E", r, logs, s1, s2)
end
print(table.concat(out6, "\n"))
end
