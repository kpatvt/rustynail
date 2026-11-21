-- K5 (`slotlive`): a store into a hash slot that the trace already proved non-nil (a typed load of the same slot, or a store of a non-nil value) takes
-- no `__newindex` guard and no old-value check. The facts end at anything that may write a nil into the slot.
-- (1) positive: field updates of an object with a class metatable, with and without `__newindex`
local C = {}
C.__index = C
function C.new() return setmetatable({ count = 0, other = 2, name = "c" }, C) end
function C:inc() self.count = self.count + self.other; self.other = self.other + 1 end
local o = C.new()
for i = 1, 400 do o:inc() end
print("class", o.count, o.other)

-- (2) a nil stored through another reference (its own store is elided: the slot was loaded just before) between the load and the store: another table
-- when the trace was recorded, the same table every fifth iteration (so the store inserts into a nil slot and __newindex runs)
local log = {}
local mt = { __newindex = function(t, k, v) log[#log + 1] = k .. "=" .. tostring(v); rawset(t, k, v) end }
local A = setmetatable({ x = 1, y = 0 }, mt)
local B = { x = 1, y = 0 }
local tabs = { B, B, B, B, A }
for i = 1, 300 do
  local u = tabs[i % 5 + 1]
  local a = A.x
  local b = u.x
  u.x = nil
  A.x = a + 1
  A.y = A.y + 1
  B.x = 7
end
print("alias nil", A.x, A.y, #log, log[1], log[#log], B.x)

-- (3) the same with the nil stored through a variable key (a `HREFV` store or a `TSET`: never a proof)
local log3 = {}
local t3 = setmetatable({ x = 1, p = 0 }, { __newindex = function(t, k, v) log3[#log3 + 1] = k; rawset(t, k, v) end })
local keys = { "p", "p", "p", "p", "x" }
for i = 1, 300 do
  local a = t3.x
  t3[keys[i % 5 + 1]] = nil
  t3.x = a + 1
  t3.p = i
end
print("dyn nil", t3.x, t3.p, #log3, log3[1])

-- (5) a table that gets a __newindex metatable inside the loop body through a call (setmetatable): the stores after it run through the metamethod
local log5 = {}
local mt5 = { __newindex = function(t, k, v) log5[#log5 + 1] = k; rawset(t, k, v) end }
local t5 = { x = 1, y = 2 }
for i = 1, 200 do
  local a = t5.x
  t5.x = nil
  t5.x = a + 1
  if i == 100 then setmetatable(t5, mt5) end
end
print("setmt", t5.x, #log5)
