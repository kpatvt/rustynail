-- F5: a field set to nil then stored again (dead node reuse); pairs / next observe the same as the interpreter.
local t = {a = 1, b = 2, c = 3, d = 4}
local n = 0
for i = 1, 500 do
  t.a = i
  if i % 3 == 0 then t.b = nil end
  t.b = i
  t.c = nil
  if i % 2 == 0 then t.c = i end
  t.d = (t.d or 0) + 1
  local cnt = 0
  for _ in pairs(t) do cnt = cnt + 1 end
  n = n + cnt
end
print(n, t.a, t.b, t.c, t.d)
local keys = {}
for k, v in pairs(t) do keys[#keys + 1] = k .. "=" .. tostring(v) end
table.sort(keys)
print(table.concat(keys, ","))
-- a metatable that never has __newindex and a nil-able field
local m = setmetatable({u = 1, v = 2}, {__index = function() return -1 end})
local s = 0
for i = 1, 500 do
  m.u = nil
  s = s + m.u
  m.u = i
  m.v = m.u + 1
end
print(s, m.u, m.v)
