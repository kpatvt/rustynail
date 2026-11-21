-- L2 (rn-core `insert_small`): the first two keys of a fresh table take a short path past the general rehash chain; the node layout, and
-- with it the `pairs` order, must stay the reference's. Every table below is printed in `pairs` order (seed-0 builds: the string hashes,
-- hence the collisions in a two-node part, are the reference's). Keys collide in about half of the pairs.
local names = {}
for i = 1, 40 do names[i] = "k" .. i end
local function order(t)
  local o = {}
  for k, v in pairs(t) do o[#o + 1] = tostring(k) .. "=" .. tostring(v) end
  return table.concat(o, " ")
end
local mt = { __index = {} }
-- 1. object idiom: setmetatable({}, mt), then two fields
local lines = {}
for i = 1, #names - 1 do
  local t = setmetatable({}, mt)
  t[names[i]] = i
  t[names[i + 1]] = -i
  lines[#lines + 1] = order(t)
end
print("pairs", table.concat(lines, " | "))
-- 2. three and four fields (the third key leaves the short path)
lines = {}
for i = 1, #names - 3, 3 do
  local t = {}
  t[names[i]] = 1; t[names[i + 2]] = 2; t[names[i + 1]] = 3
  local u = {}
  u[names[i + 3]] = 1; u[names[i]] = 2; u[names[i + 1]] = 3; u[names[i + 2]] = 4
  lines[#lines + 1] = order(t) .. " / " .. order(u)
end
print("three", table.concat(lines, " | "))
-- 3. a one-node constructor part holding an integer / float / boolean / string key, then a string key
lines = {}
for i = 1, 20 do
  local a = { [i % 4 + 1] = true }
  a[names[i]] = i
  local b = { [i + 0.5] = true }
  b[names[i]] = i
  local c = { [i % 2 == 0] = i }
  c[names[i]] = i
  local d = { [names[i + 1]] = i }
  d[names[i]] = -i
  lines[#lines + 1] = order(a) .. " # " .. #a .. " ; " .. order(b) .. " ; " .. order(c) .. " ; " .. order(d)
end
print("ctor", table.concat(lines, " | "))
-- 4. deletions: a deleted entry is reused in place, a nil store into an empty table inserts nothing
lines = {}
for i = 1, 20 do
  local t = {}
  t[names[i]] = 1; t[names[i]] = nil; t[names[i + 1]] = 2; t[names[i + 2]] = 3
  local u = {}
  u[names[i]] = nil; u[names[i + 3]] = 4; u[names[i + 3]] = nil; u[names[i + 4]] = 5; u[names[i]] = 6
  lines[#lines + 1] = order(t) .. " / " .. order(u)
end
print("del", table.concat(lines, " | "))
-- 5. an array part first (the short path refuses it), integer keys after strings, rawset
lines = {}
for i = 1, 20 do
  local t = {}
  t[1] = 1; t[names[i]] = 2; t[names[i + 1]] = 3
  local u = {}
  u[names[i]] = 1; u[1] = 2; u[2] = 3; u[names[i + 1]] = 4
  local w = setmetatable({}, { __newindex = rawset })
  w[names[i + 2]] = 1; w[names[i]] = 2
  lines[#lines + 1] = order(t) .. " / " .. order(u) .. " # " .. #u .. " / " .. order(w)
end
print("mixed", table.concat(lines, " | "))
-- 6. a hot loop (the trace's `NEWREF` reaches the same path)
local acc = {}
for r = 1, 600 do
  local i = r % 39 + 1
  local t = setmetatable({}, mt)
  t[names[i]] = r
  t[names[(i * 7) % 40 + 1]] = -r
  if r > 560 then acc[#acc + 1] = order(t) end
end
print("hot", table.concat(acc, " | "))
