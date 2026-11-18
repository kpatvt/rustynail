-- F5: the metatable table itself gains / loses __newindex mid-loop (flag cache of the metatable).
local log = {}
local mt = {}
local t = setmetatable({a = 1, b = 2}, mt)
for i = 1, 600 do
  if i == 150 then mt.__newindex = function(tt, k, v) log[#log + 1] = k .. v; rawset(tt, k, v) end end
  if i == 300 then rawset(mt, "__newindex", nil) end
  if i == 450 then rawset(mt, "__newindex", function(tt, k, v) log[#log + 1] = "B" .. k .. v; rawset(tt, k, v) end) end
  t.a = i
  if i % 5 == 0 then t.b = nil end
  t.b = i
end
print(t.a, t.b, #log, log[1], log[#log])
-- a __newindex table inserted into the metatable
local sink = {}
local mt2 = {}
local v = setmetatable({k = 1}, mt2)
for i = 1, 400 do
  if i == 200 then mt2.__newindex = sink end
  if i % 3 == 0 then v.k = nil end
  v.k = i
end
print(v.k, sink.k, rawget(v, "k"))
-- integer keys in the hash part
local mt3 = {}
local w = setmetatable({[1000] = 1, [2000] = 2}, mt3)
local lg = {}
for i = 1, 400 do
  if i == 100 then mt3.__newindex = function(tt, k, vv) lg[#lg + 1] = k; rawset(tt, k, vv) end end
  if i == 250 then mt3.__newindex = nil end
  w[1000] = i
  if i % 4 == 0 then w[2000] = nil end
  w[2000] = i
end
print(w[1000], w[2000], #lg)
