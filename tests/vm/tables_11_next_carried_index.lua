-- F1: `next` / `pairs` with the carried traversal index: order, assigning nil to existing fields during traversal, new keys of other
-- tables between steps (a stale hint), nested traversals, rehash between steps, a table that changed identity, float / boolean / table
-- / string keys, and the errors of `next`.
local function keys(t) local r = {} for k, v in pairs(t) do r[#r + 1] = tostring(k) .. "=" .. tostring(v) end return table.concat(r, ",") end
local h = {}
for i = 1, 40 do h["k" .. i] = i end
for i = 1, 10 do h[i * 1.5] = i end
h[true] = "t"; h[false] = "f"
local function trav(n)
  local s = 0
  for r = 1, n do
    for k, v in pairs(h) do if type(v) == "number" then s = s + v end end
  end
  return s
end
print(trav(50), #keys(h))
-- assigning nil to existing fields during the traversal
local d = {}
for i = 1, 30 do d["a" .. i] = i; d[i + 100] = i end
local cnt, sum = 0, 0
for k, v in pairs(d) do d[k] = nil; cnt = cnt + 1; sum = sum + v end
print(cnt, sum, next(d))
-- updating existing fields during the traversal
local u = {}
for i = 1, 20 do u["u" .. i] = i end
for k, v in pairs(u) do u[k] = v * 2 end
local tot = 0
for k, v in pairs(u) do tot = tot + v end
print(tot)
-- two interleaved traversals of two tables (the hint belongs to the last one)
local A, B = {}, {}
for i = 1, 25 do A["x" .. i] = i; B["y" .. i] = -i end
local ka, kb, n, sa, sb = nil, nil, 0, 0, 0
while true do
  local va, vb
  ka, va = next(A, ka)
  kb, vb = next(B, kb)
  if ka == nil and kb == nil then break end
  n = n + 1; sa = sa + (va or 0); sb = sb + (vb or 0)
end
print(n, sa, sb)
-- nested traversals of the same and of different tables
local c = 0
for k1 in pairs(A) do for k2 in pairs(A) do c = c + 1 end for k3 in pairs(B) do c = c + 1 end end
print(c)
-- a traversal that is interrupted by a rehash of the table (inserting new keys is undefined; only existing-key assignment is not)
local r = {}
for i = 1, 8 do r["r" .. i] = i end
local before = keys(r)
for i = 1, 64 do r["n" .. i] = i end
for i = 1, 64 do r["n" .. i] = nil end
print(before == keys(r) or "order differs", next({}), next({}, nil))
-- errors
print(pcall(next, {}, "nokey"))
print(pcall(next, {10, 20}, 3))
print(pcall(next, {10, 20}, 2))
print(pcall(next, h, {}))
print(select("#", next({})))
-- keys of every type, array + hash, integer keys beyond the array, negative and zero keys
local m = {10, 20, 30, [0] = "z", [-1] = "n", [1000] = "big", [2^53] = "f", [1.5] = "h", [true] = 1}
print(keys(m))
local last
for k in pairs(m) do last = k end
print(next(m, last))
-- next with a key that was removed (assigned nil) and then asked for
local rm = {}
for i = 1, 10 do rm["q" .. i] = i end
local firstk = next(rm)
rm[firstk] = nil
print((pcall(next, rm, firstk)))
collectgarbage()
print((pcall(next, rm, firstk)))
-- a loop long enough to be compiled, with a table switch halfway
local P, Q = {}, {}
for i = 1, 50 do P["p" .. i] = i; Q["q" .. i] = i * 2 end
local function walk(n)
  local s = 0
  for r = 1, n do
    local t = (r % 3 == 0) and Q or P
    for k, v in pairs(t) do s = s + v end
    for k, v in next, t do s = s + v end
  end
  return s
end
print(walk(200))
local function walk2(n)
  local s = 0
  local t = {}
  for i = 1, 20 do t["w" .. i] = i end
  for r = 1, n do
    for k, v in pairs(t) do
      if r % 5 == 0 then t[k] = v + 1 end
      s = s + v
    end
    if r == n // 2 then for i = 1, 20, 2 do t["w" .. i] = nil end end
  end
  return s
end
print(walk2(300))
