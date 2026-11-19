-- table.move: overlapping ranges in both directions, one table / two tables, metamethods, edge indices, hot loops
local move = table.move
local function dump(t, n)
  local r = {}
  for i = 1, n do r[i] = tostring(t[i]) end
  return table.concat(r, ",")
end
local function seq(n)
  local t = {}
  for i = 1, n do t[i] = i end
  return t
end

-- dst > src (backwards copy), dst < src (forwards copy), dst == src, disjoint
for _, c in ipairs { { 2, 6, 3 }, { 3, 7, 2 }, { 1, 5, 4 }, { 4, 8, 1 }, { 3, 7, 3 }, { 1, 4, 7 }, { 7, 10, 1 }, { 1, 10, 2 }, { 2, 10, 1 } } do
  local t = seq(12)
  local r = move(t, c[1], c[2], c[3])
  print(c[1], c[2], c[3], r == t, dump(t, 12))
end
-- different tables, destination given and equal to the source
local a, b = seq(8), { 0, 0, 0, 0, 0, 0, 0, 0 }
print(dump(move(a, 2, 6, 3, b), 8), dump(a, 8))
print(dump(move(a, 2, 6, 3, a), 8))
print(dump(move(a, 3, 7, 2, nil), 8))
-- empty and one-element ranges, negative / zero indices
print(dump(move(seq(5), 3, 2, 1), 5), dump(move(seq(5), 1, 0, 3), 5), dump(move(seq(5), 2, 2, 4), 5))
do
  local t = { [0] = "z", [-1] = "y", 1, 2, 3 }
  move(t, -1, 1, 0)
  print(t[-1], t[0], t[1], t[2], t[3])
  move(t, 0, 3, -1)
  print(t[-1], t[0], t[1], t[2], t[3])
end
-- the edges of the integer range
local M, m = math.maxinteger, math.mininteger
do
  local t = {}
  move({ 1, 2, 3 }, 1, 3, M - 2, t)
  print(t[M - 2], t[M - 1], t[M])
  move(t, M - 2, M, 1)
  print(t[1], t[2], t[3])
  print(pcall(move, { 1 }, 1, M, 2))
  print(pcall(move, { 1 }, 0, M, 1))
  print(pcall(move, { 1 }, -1, M, 1))
  print(pcall(move, { 1 }, 1, 2, M))
  print(pcall(move, { 1 }, m, 0, 1))
  print(pcall(move, { 1 }, m, -1, 1))
  local u = {}
  move({ 5 }, 1, 1, m, u)
  print(u[m])
  print(pcall(move, {}, 1, 1))
end
-- __index / __newindex are honoured, in the order of the copy
do
  local log = {}
  local src = setmetatable({}, { __index = function(_, k) log[#log + 1] = "g" .. k return k * 10 end })
  local dst = setmetatable({}, { __newindex = function(t, k, v) log[#log + 1] = "s" .. k rawset(t, k, v) end })
  move(src, 1, 3, 2, dst)
  print(table.concat(log, " "), dump(dst, 4))
  log = {}
  local same = setmetatable({ 1, 2, 3, 4 }, {
    __index = function(_, k) log[#log + 1] = "g" .. k end,
    __newindex = function(t, k, v) log[#log + 1] = "s" .. k rawset(t, k, v) end,
  })
  move(same, 1, 4, 3)
  print(table.concat(log, " "), dump(same, 6))
  log = {}
  move(same, 3, 6, 1)
  print(table.concat(log, " "), dump(same, 6))
end
-- hot loops: the same moves are compiled by the JIT; counts above, at and below the unrolled limit
local function hot(n, f, l, d)
  local t = seq(20)
  for i = 1, n do move(t, f, l, d) end
  return dump(t, 20)
end
for _, n in ipairs { 1, 2, 3, 300, 301, 1000 } do
  print(n, hot(n, 2, 6, 3), hot(n, 3, 7, 2), hot(n, 1, 5, 4), hot(n, 4, 8, 1))
  print(n, hot(n, 1, 16, 2), hot(n, 2, 17, 1), hot(n, 1, 17, 2), hot(n, 5, 6, 19))
end
local function hot2(n)
  local t = seq(10)
  for i = 1, n do move(t, 2, 6, 3) move(t, 3, 7, 2) end
  return dump(t, 10)
end
print(hot2(901), hot2(900), hot2(7))
-- a type mark travelling through the range (float, string, boolean, table)
for _, mark in ipairs { 0.5, "m", true, false } do
  for pos = 1, 9 do
    local t = seq(10)
    t[pos] = mark
    for i = 1, 400 do move(t, 2, 6, 3) move(t, 3, 7, 2) end
    print(tostring(mark), pos, dump(t, 10))
  end
end
-- growth: the destination reaches beyond the array part
local function grow(n)
  local t = seq(6)
  for i = 1, n do move(t, 2, 6, 3) t[1] = i end
  return dump(t, 9)
end
print(grow(700), grow(701))
