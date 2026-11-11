local function f(...) return select('#', ...), ... end
local function g(...) return f(...) end
print(g(1, 2, 3))
local function p() return pcall(error, "x") end
print(p())
local function tp(...) return table.unpack({...}) end
print(tp(1, 2, 3))
local function ts(x) return tostring(x) end
print(ts(5), ts(nil))
local function ty(x) return type(x) end
print(ty(5), ty({}), ty(print))
local function err() return error("tail err") end
print(pcall(err))
local function sel() return select(2, "a", "b", "c") end
print(sel())
