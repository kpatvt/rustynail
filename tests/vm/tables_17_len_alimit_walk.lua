
-- `#t` on tables whose array limit (5.4 `alimit`) moves: pushes, pops, stores with holes, tables of several array sizes, the length read
-- inside hot loops (trace and tier 1) and between the stores. No key outside the array part (the hash search is seed dependent on 5.5).
local function mk(n, sz)
  local t = {}
  for i = 1, n do t[i] = i end
  return t
end
local function walk(t, steps, seed)
  local x, acc = seed, 0
  for s = 1, steps do
    x = (x * 1103515245 + 12345) % 2147483648
    local op = (x >> 8) % 8
    local n = #t
    acc = (acc * 31 + n) % 1000000007
    if op < 3 then
      if n < 14 then t[n + 1] = s end
    elseif op < 5 then
      if n > 0 then t[n] = nil end
    elseif op == 5 then
      local k = (x >> 12) % 15 + 1
      if k <= 16 then t[k] = (t[k] == nil) and s or nil end
    else
      acc = acc + #t
    end
  end
  return acc
end
local out = {}
for _, n in ipairs { 0, 1, 3, 4, 5, 7, 8, 9, 12, 13, 16 } do
  for seed = 1, 6 do
    local t = mk(n)
    out[#out + 1] = walk(t, 150, seed * 7 + n) .. ":" .. #t
  end
end
print(table.concat(out, " "))
-- the length of a full array, a half-full one and a table with a constructor hole, read from a hot loop
local function rd(t, n) local a = 0 for i = 1, n do a = a + #t end return a end
local full, half, hole = { 1, 2, 3, 4, 5, 6, 7, 8 }, mk(5), { 1, 2, nil, 4, 5, nil, nil, 8 }
half[8] = nil
local r = {}
for k = 1, 60 do r[#r + 1] = rd(full, 20) + rd(half, 20) + rd(hole, 20) + #{ n = 1 } + #{} end
print(r[1], r[60], #full, #half, #hole)
