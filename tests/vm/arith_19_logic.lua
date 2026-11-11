print(nil and 1, false and 1, 0 and 1, "" and 1, 1 and nil, 1 and false)
print(nil or 1, false or 1, 0 or 1, "" or 1, nil or false, false or nil)
print(1 and 2 or 3, nil and 2 or 3, false or false and 1, nil or nil or 5)
local a, b = nil, 0
print(a and a.x, b and b + 1, a or b, a == nil and "nil" or "notnil")
local function f(x) return x and "t" or "f" end
print(f(nil), f(false), f(0), f(""), f({}))
print(not not nil, not not 0, not (1 == 2), not (nil == false))
local c = 0
local function inc() c = c + 1 return c end
local r = inc() > 0 and inc() > 1 and inc()
print(r, c)
r = inc() < 0 and inc()
print(r, c)
r = inc() < 0 or inc()
print(r, c)
