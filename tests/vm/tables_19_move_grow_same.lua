-- table.move within one table whose destination range grows the array (overlapping, forwards), from a hot loop: the recorder reads the
-- sources before the first store and copies those values (a re-read after the insertion, which resyncs the lockstep model from the
-- finished call, would see moved elements). Found by jit-diff set `br` (+verify), seeds 61 / 135 / 183.
-- (a first move loop into another table, as in the generated program: its trace precedes the one of run5)
local function run3(n)
  local src = {}
  for j = 1, 7 do src[j] = j * 3 end
  local dst, acc = {}, 0
  for i = 1, n do
    local f, e, d = 2, 7, 1
    if i % 23 == 0 then d = d + 2 end
    table.move(src, f, e, d, dst)
    acc = acc + (dst[d] or 0) + (src[e] or 0) + #dst
  end
  return acc, #dst
end
print(run3(273))
local units4 = { "a", "ab", "xyz", "", ("q"):rep(7) }
local seps4 = { "", ",", ", ", "--" }
local function run4(n)
  local acc, ls = 0, 0
  for i = 1, n do
    local u, p = units4[i % #units4 + 1], seps4[(i // 2) % #seps4 + 1]
    local c = (i * 7) % 36
    local r = string.rep(u, c)
    local r2 = string.rep(u, c, p)
    acc = acc + #r + #r2 * 3
    if #r2 > 40 then ls = ls + 1 end
    if i % 41 == 0 then acc = acc + #string.rep("ab", 1 << 12) end
  end
  return acc, ls
end
print(run4(302))
local function run5(n)
  local src = {}
  for j = 1, 7 do src[j] = j * 3 end
  local dst, acc = {}, 0
  for i = 1, n do
    local f, e, d = 3, 7, 5
    if i % 31 == 0 then f = f + 1 end
    table.move(src, f, e, d, src)
    acc = acc + (dst[d] or 0) + (src[e] or 0) + #dst
  end
  local out = {}
  for j = 1, 7 + 4 do out[#out + 1] = tostring(dst[j]) .. "/" .. tostring(src[j]) end
  return acc, table.concat(out, ",")
end
print(run5(397))
-- the same with a table that has spare capacity and a move that stays inside / reaches past it, backwards overlap
local function run6(n, d)
  local acc = 0
  for i = 1, n do
    local t = {}
    for j = 1, 9 do t[j] = j * 7 end
    table.move(t, 2, 8, d, t)
    acc = acc + #t + (t[d] or 0) + (t[d + 6] or 0)
  end
  return acc
end
print(run6(120, 4), run6(120, 3), run6(120, 10), run6(120, 12))
