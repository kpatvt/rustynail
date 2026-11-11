local mt = {}
mt.__lt = function(a, b) return a.v < b.v end
mt.__le = function(a, b) return a.v <= b.v end
mt.__eq = function(a, b) return a.v == b.v end
local function new(v) return setmetatable({v = v}, mt) end
local a, b, c = new(1), new(2), new(1)
print(a < b, a <= b, a > b, a >= b, a == c, a ~= c, a == b, rawequal(a, c))
print(pcall(function() return a < 1 end))
print(pcall(function() return 1 < a end))
print(a == 1, 1 == a, a == nil, a == "x")
local only_lt = setmetatable({}, {__lt = function() return true end})
print(only_lt < only_lt, pcall(function() return only_lt <= only_lt end))
local eq_count = 0
local e1 = setmetatable({}, {__eq = function() eq_count = eq_count + 1 return true end})
local e2 = setmetatable({}, {__eq = function() eq_count = eq_count + 1 return false end})
print(e1 == e2, e2 == e1, e1 == e1, eq_count)
local eqany = setmetatable({}, {__eq = function() return 1 end})
print(eqany == {}, eqany ~= {})
local lt_nonbool = setmetatable({}, {__lt = function() return nil end, __le = function() return "x" end})
print(lt_nonbool < lt_nonbool, lt_nonbool <= lt_nonbool, lt_nonbool > lt_nonbool)
