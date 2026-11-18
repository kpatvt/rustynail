-- F5: a store whose old value is the nil that the previous iteration stored (the recorded iteration saw a non-nil old value),
-- into a table with __newindex (function / table): the old-value check must stay, every insertion into the nil slot reaches
-- the metamethod.
local log = {}
local t = setmetatable({q = 1, r = 0}, {__newindex = function(tt, k, v) log[#log + 1] = k .. ":" .. tostring(v); rawset(tt, k, v) end})
for i = 1, 300 do
  t.q = i
  t.r = t.r + 1
  t.q = nil
end
print(rawget(t, "q"), t.r, #log, log[1], log[#log])
-- the same through a __newindex table
local sink = {}
local u = setmetatable({q = 1, r = 0}, {__newindex = sink})
for i = 1, 300 do
  u.q = i
  u.r = u.r + 1
  u.q = nil
end
print(rawget(u, "q"), u.r, sink.q, rawget(sink, "r"))
-- a metamethod-name key (the flag cache must not be trusted for it) with __newindex installed before the loop
local log2 = {}
local mt = {}
local w = setmetatable({__q = 1, r = 0}, mt)
mt.__newindex = function(tt, k, v) log2[#log2 + 1] = k; rawset(tt, k, v) end
for i = 1, 300 do
  w.__q = i
  w.r = w.r + 1
  w.__q = nil
end
print(rawget(w, "__q"), w.r, #log2)
