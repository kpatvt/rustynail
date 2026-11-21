-- K5 (`slotlive`): a store into an array slot that the trace already proved non-nil (a typed `ALOAD` of the same element, or a store of a non-nil value) takes
-- no metatable guard and no old-value guard. Another index, another table, or a store that may write a nil ends the fact.
-- (1) the awfy permute swap, through a method and `self.v`, with and without a metatable on the array
local P = {}
P.__index = P
function P:swap(i, j)
  local tmp = self.v[i + 1]
  self.v[i + 1] = self.v[j + 1]
  self.v[j + 1] = tmp
end
local p = setmetatable({ v = { 1, 2, 3, 4, 5, 6 } }, P)
for r = 1, 300 do p:swap(r % 6, (r * 5) % 6) end
print("swap", table.concat(p.v, ","))
local logs = {}
local q = setmetatable({ v = setmetatable({ 1, 2, 3, 4, 5, 6 }, { __newindex = function(t, k, v) logs[#logs + 1] = k; rawset(t, k, v) end }) }, P)
for r = 1, 300 do q:swap(r % 6, (r * 5) % 6) end
print("swap mt", table.concat(q.v, ","), #logs)

-- (2) a load of element j, a store to element j + 1 that is nil at run time in some iterations: the store inserts, so __newindex runs
local log2 = {}
local t2 = setmetatable({ 1, 2, 3, nil, 5, 6, nil, 8 }, { __newindex = function(t, k, v) log2[#log2 + 1] = k .. "=" .. tostring(v); rawset(t, k, v) end })
for i = 1, 300 do
  local j = i % 7 + 1
  local a = t2[j]
  if a then t2[j + 1] = a end
end
print("next index", table.concat({ t2[1], t2[2], t2[3], t2[4], t2[5], t2[6], t2[7], t2[8] }, ","), #log2, log2[1], log2[#log2])
-- the same without a metatable (the guard that goes away is `metatable == 0`: no observable difference, the result must still agree)
local t2b = { 1, 2, 3, nil, 5, 6, nil, 8 }
for i = 1, 300 do
  local j = i % 7 + 1
  local a = t2b[j]
  if a then t2b[j + 1] = a end
end
print("next index plain", t2b[1], t2b[2], t2b[3], t2b[4], t2b[5], t2b[6], t2b[7], t2b[8])

-- (3) the table of the store is loaded again after a store changed the field it comes from: not the table of the earlier load (the new table is
-- another one every fifth iteration: one with a __newindex metatable whose element is nil)
local log3 = {}
local B1 = { 0, 0, 0 }
local B2 = setmetatable({ nil, 0, 0 }, { __newindex = function(t, k, v) log3[#log3 + 1] = k .. "=" .. tostring(v) end })
local Bs = { B1, B1, B1, B1, B2 }
local A = { 10, 20, 30 }
local o = { v = A }
for i = 1, 300 do
  local x = o.v[1]
  o.v = Bs[i % 5 + 1]
  o.v[1] = x
  o.v = A
end
print("reloaded table", A[1], B1[1], rawget(B2, 1), #log3, log3[1])

-- (4) a nil stored into the slot in between through the same table and another index expression (a variable index)
local log4 = {}
local t4 = setmetatable({ 1, 2, 3, 4 }, { __newindex = function(t, k, v) log4[#log4 + 1] = k; rawset(t, k, v) end })
local idx = { 3, 3, 3, 3, 2 }
for i = 1, 300 do
  local a = t4[2]
  t4[idx[i % 5 + 1]] = nil
  t4[2] = (a or 0) + 1
  t4[3] = i
end
print("var index nil", t4[2], #log4)
