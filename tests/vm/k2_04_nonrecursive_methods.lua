-- K2: methods with calls whose invocations are mostly leaves but that never recurse, called from a Lua function of another prototype:
-- no earlier activation of the same prototype is on the stack, so the entry head keeps the normal `FnShort` budget
local Q = {}
Q.__index = Q
function Q:aux(x) self.m = self.m + x return self.m end
function Q:hold(x)
  self.n = self.n + 1
  if x > 0 then return self.n end
  return self:aux(x)
end
function Q:relay(x)
  if x % 7 ~= 0 then return x end
  return self:hold(x) + 1
end
local q = setmetatable({ n = 0, m = 0 }, Q)
local function drive(n)
  local t = 0
  for i = 1, n do t = t + q:hold(i % 17 - 3) + q:relay(i % 11) end
  return t
end
local total = 0
for i = 1, 800 do total = total + drive(100) end
print("hold", total, q.n, q.m)
