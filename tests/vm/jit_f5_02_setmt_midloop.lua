-- F5: the metatable of the stored-to table changes between iterations of a hot loop: __newindex added, removed, replaced.
local seen = {}
local t = {x = 1, y = 2, z = 3}
local mtA = {__newindex = function(tt, k, v) seen[#seen + 1] = "A" .. k .. v; rawset(tt, k, v) end}
local mtB = {}
for i = 1, 600 do
  if i == 100 then setmetatable(t, mtB) end
  if i == 200 then setmetatable(t, mtA) end
  if i == 300 then setmetatable(t, nil) end
  if i == 400 then setmetatable(t, mtA) end
  if i == 450 then setmetatable(t, mtB) end
  t.x = i
  if i % 7 == 0 then t.y = nil end
  t.y = (t.y or 0) + 1
  t.z = i * 0.5
end
print(t.x, t.y, t.z, #seen, seen[1], seen[#seen])
-- the call that changes the metatable sits between two stores of the same table
local u = {p = 0, q = 0}
local log = {}
local mt2 = {__newindex = function(tt, k, v) log[#log + 1] = k .. ":" .. tostring(v); rawset(tt, k, v) end}
local function flip(i)
  if i % 40 == 20 then setmetatable(u, mt2) elseif i % 40 == 0 then setmetatable(u, nil) end
end
for i = 1, 500 do
  u.p = i
  flip(i)
  u.q = nil
  u.q = i
  u.p = i + 1
end
print(u.p, u.q, #log, log[1], log[#log])
