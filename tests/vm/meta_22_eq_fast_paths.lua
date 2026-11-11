-- T2: EQ on tables without `__eq` (no metatable / metatable without the event, absence cached in the
-- flags) and EQK against short/long string, integer, float and boolean constants.
local out = {}
local function P(...)
  local t = {}
  for i = 1, select("#", ...) do t[i] = tostring((select(i, ...))) end
  out[#out + 1] = table.concat(t, " ")
end

local function eq(a, b) return a == b end
local function ne(a, b) return a ~= b end
local function ifeq(a, b) if a == b then return "y" else return "n" end end

local a, b = {}, {}
P("plain", tostring(a == b), tostring(a == a), tostring(a ~= b), ifeq(a, b), ifeq(a, a))
local mt = {}
local c, d = setmetatable({}, mt), setmetatable({}, mt)
P("mt-no-eq", tostring(c == d), tostring(c == c), tostring(c == a), tostring(a == c), ne(c, d) and "ne" or "eq")

-- __eq added later: the cached absence in the metatable flags must be invalidated
mt.__eq = function(x, y) return true end
P("eq-added", tostring(c == d), tostring(d == c), tostring(c == a), tostring(a == c), tostring(a == b))
mt.__eq = nil
P("eq-removed", tostring(c == d), tostring(c == a))
mt.__eq = function() return 1 end
P("eq-truthy", tostring(c == d), tostring(c ~= d))
mt.__eq = false
local okf, ef = pcall(eq, c, d)
P("eq-false", tostring(okf), (tostring(ef):gsub("^.-:%d+: ", "")))
mt.__eq = nil

-- __eq on the second operand only, different metatables, rawequal
local m1, m2 = {}, {__eq = function() return true end}
local e1, e2 = setmetatable({}, m1), setmetatable({}, m2)
P("second-only", tostring(e1 == e2), tostring(e2 == e1), tostring(rawequal(e1, e2)))
-- the result of __eq is converted to boolean, and not called for the same object
local calls = 0
local m3 = {__eq = function() calls = calls + 1; return nil end}
local f1, f2 = setmetatable({}, m3), setmetatable({}, m3)
P("calls", tostring(f1 == f2), tostring(f1 == f1), calls)
-- in a loop (hot enough to be compiled by the tier-1 JIT)
local hits = 0
local g1, g2 = setmetatable({}, m3), setmetatable({}, m3)
for i = 1, 300 do
  if g1 == g2 then hits = hits + 1 end
  if i == 150 then m3.__eq = function() return true end end
end
P("loop", hits)
-- userdata (io files) keep going through the metamethod path
P("udata", tostring(io.stdout == io.stdout), tostring(io.stdout == io.stderr))

-- EQK: constants of every kind
local function k1(x) return x == "hello" end
local function k2(x) return x == "a long string constant that is certainly longer than forty characters" end
local function k3(x) return x == 10 end
local function k4(x) return x == 2.5 end
local function k5(x) return x == true end
local function k6(x) return x == nil end
local function k7(x) return x == 1e100 end
local long = "a long string constant that is certainly longer than " .. "forty characters"
local hello = "hel" .. "lo"
P("k-str", tostring(k1("hello")), tostring(k1(hello)), tostring(k1("hellp")), tostring(k1(1)), tostring(k1({})))
P("k-long", tostring(k2(long)), tostring(k2(long .. "x")), tostring(k2("hello")), tostring(k2(1)))
P("k-int", tostring(k3(10)), tostring(k3(10.0)), tostring(k3(11)), tostring(k3("10")), tostring(k3(nil)))
P("k-flt", tostring(k4(2.5)), tostring(k4(5 / 2)), tostring(k4(2)), tostring(k4(0 / 0)))
P("k-bool", tostring(k5(true)), tostring(k5(false)), tostring(k5(1)), tostring(k5(nil)))
P("k-nil", tostring(k6(nil)), tostring(k6(false)), tostring(k6(0)))
P("k-big", tostring(k7(1e100)), tostring(k7(2 ^ 332)), tostring(k7(math.huge)))
local n = 0
for i = 1, 400 do
  if k1(i % 2 == 0 and "hello" or "world") then n = n + 1 end
  if k3(i) then n = n + 1000 end
end
P("loop-k", n)

print(table.concat(out, "\n"))
