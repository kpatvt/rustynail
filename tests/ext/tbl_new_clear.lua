local C = dofile("_common.lua")
local P, show = C.P, C.show
local new, clear = require("table.new"), require("table.clear")
print("same", new == table.new, clear == table.clear)
print("type", type(new), type(clear))

-- table.new
local t = new(4, 2)
print("new empty", #t, next(t))
t = new(0, 0); print("new 0,0", #t, next(t))
t = new(8, 0)
for i = 1, 8 do t[i] = i * 2 end
print("fill", #t, show(t))
t = new(0, 8)
for i = 1, 8 do t["k" .. i] = i end
print("hash fill", show(t))
t = new(3.9, 2.9); print("trunc", #t)
P("none", new)
P("one", new, 3)
P("str", new, "x", 1)
P("strnum", new, "3", "2")
P("huge", new, 1e18, 0)
P("huge2", new, 0, 1e18)
P("nan", new, 0 / 0, 0)
P("inf", new, 1 / 0, 0)
P("table", new, {}, 0)

-- table.clear
t = { 1, 2, 3, a = 1, b = 2 }
print("clear ret", select("#", clear(t)), next(t), #t)
t = setmetatable({ 1, 2, 3, a = 1 }, { __index = function(_, k) return "mt" .. tostring(k) end })
clear(t)
print("clear mt", t[1], t.a, getmetatable(t) ~= nil, rawget(t, 1), next(t))
t[1] = "again"; t.x = "y"
print("reuse", show(t))
local log = {}
t = setmetatable({ 1, 2 }, { __newindex = function() log[#log + 1] = "ni" end, __index = function() log[#log + 1] = "i" end })
clear(t)
print("raw", #log)
P("noargs", clear)
P("num", clear, 1)
P("str", clear, "s")
P("nil", clear, nil)
P("extra", clear, {}, 1)

-- clear during traversal
t = { 1, 2, 3, x = 1, y = 2 }
for k, v in pairs(t) do clear(t) end
print("trav", next(t))
t = { 1, 2, 3, x = 1, y = 2 }
local n = 0
for k, v in pairs(t) do n = n + 1; if n == 2 then clear(t) end end
print("trav2", n, next(t))
-- clearing a shared structure
t = {}
for i = 1, 100 do t[i] = i; t["s" .. i] = i end
clear(t)
print("big", #t, next(t))
for i = 1, 100 do t[i] = -i end
print("big refill", #t, t[100], t.s1)
-- many cycles keep working (reuse pattern)
t = new(16, 16)
local sum = 0
for round = 1, 200 do
  for i = 1, 16 do t[i] = i + round; t["k" .. i] = i end
  for i = 1, 16 do sum = sum + t[i] + t["k" .. i] end
  clear(t)
end
print("cycles", sum, next(t))
-- collectgarbage interplay
t = new(4, 4)
for i = 1, 4 do t[i] = { i }; t["k" .. i] = { i } end
clear(t)
collectgarbage(); collectgarbage()
print("gc", next(t))
for i = 1, 4 do t[i] = { i }; t["k" .. i] = { i } end
collectgarbage()
print("gc2", #t, t.k4[1])
