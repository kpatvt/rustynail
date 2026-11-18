-- F5: a call (setmetatable) between two stores of one table changes its metatable: the __newindex guard of the first store
-- must not be reused for the second.
local log = {}
local A = {}
local B = {__newindex = function(tt, k, v) log[#log + 1] = k .. ":" .. tostring(v); rawset(tt, k, v) end}
local mts = {A, B}
local u = {p = 0, q = 0}
-- prime the flag cache of A (an insertion looks up __newindex)
setmetatable(u, A)
u.tmp = 1
u.tmp = nil
local n = 0
for i = 1, 400 do
  u.p = i
  setmetatable(u, mts[(i // 20) % 2 + 1])
  u.q = i
  n = n + 1
  u.q = nil
end
print(n, u.p, rawget(u, "q"), #log, log[1], log[#log])
-- no metatable at all before the call
local v = {p = 0, q = 0}
local log2 = {}
local C = {__newindex = function(tt, k, val) log2[#log2 + 1] = k; rawset(tt, k, val) end}
local mts2 = {C, false}
for i = 1, 400 do
  v.p = i
  local m = mts2[(i // 20) % 2 + 1]
  setmetatable(v, m or nil)
  v.q = i
  v.q = nil
end
print(v.p, rawget(v, "q"), #log2)
