-- 5.4 array limit (`alimit`) touches in hot loops: the same loop run on tables whose limit is hinted (BITRAS set by an earlier `#t`) and on
-- tables whose limit is the array size, a loop whose own `#t` sets the hint after the first accesses, and the border `#t` reports afterwards.
-- (The trace compiler records the access of a BITRAS-clear table as a guard; an exit at a table with the bit set must leave `alimit` as the
-- interpreter would.) Arrays only, sizes below 64.
local function sum(t, n)
  local s = 0
  for i = 1, n do s = s + t[i] end
  return s
end
local function fill(t, n, v)
  for i = 1, n do t[i] = v + i end
end
local function mk(n)
  local t = {}
  for i = 1, n do t[i] = i end
  return t
end
-- tables with holes: `#t` moves the limit inside the array and sets the bit
local function holey()
  local t = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16 }
  t[16], t[15], t[14] = nil, nil, nil
  return t
end
local out = {}
for round = 1, 40 do
  local a, b, c = mk(16), holey(), mk(13)
  if round % 3 == 0 then local _ = #b end -- b's bit set before the loops
  if round % 4 == 0 then local _ = #c end
  local acc = 0
  for rep = 1, 30 do
    acc = acc + sum(a, 16) + sum(b, 13) + sum(c, 13)
    fill(a, 16, rep)
    fill(c, 13, rep)
    if rep == 20 then local _ = #b end -- the bit appears in the middle of the loop
  end
  b[14] = 99
  out[#out + 1] = acc .. ":" .. #a .. ":" .. #b .. ":" .. #c
end
print(table.concat(out, " "))
-- the loop body itself asks for the length (the bit flips during the recorded iteration)
local function drain(t, n)
  local v = 0
  for j = 1, n do v = v + table.remove(t) end
  return v
end
local r = {}
for k = 1, 80 do
  local t = mk(32)
  t[32] = k
  r[#r + 1] = drain(t, 32) + #t
end
print(r[1], r[80], #r)
local function push(t, n, base)
  for j = 1, n do t[#t + 1] = base + j end
end
local q = {}
for k = 1, 60 do
  local t = {}
  push(t, 20, k)
  t[12] = nil
  r[#r + 1] = #t + sum(t, 11)
end
print(r[81], r[140], #r)
-- stores above a hinted limit: the touches raise it, so the border `#t` finds afterwards depends on them (a hole above the stored run)
local function run(t, n)
  for i = 11, n do if i ~= 13 then t[i] = i end end
end
local function fresh()
  local t = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16 }
  for i = 10, 16 do t[i] = nil end
  return t
end
local z = {}
for k = 1, 60 do
  local t, u = fresh(), fresh()
  if k % 2 == 0 then local _ = #t end -- the limit becomes 9 and the bit is set (the border is above half of the array)
  if k % 3 == 0 then local _ = #u end
  run(t, 15)
  run(u, 15)
  z[#z + 1] = #t .. "/" .. #u
end
print(table.concat(z, " "))
