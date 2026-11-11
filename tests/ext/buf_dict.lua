-- `dict` (string dictionary for table keys) and `metatable` (class tables by index) options of buffer.new.
local C = dofile("_common.lua")
local P, hex, show = C.P, C.hex, C.show
local buf = require("string.buffer")

local function enc(opts, v) local b = buf.new(opts) b:encode(v) return hex(b) end
print("dict1", enc({ dict = { "x", "y", "foo" } }, { x = 1 }))
print("dict2", enc({ dict = { "x", "y", "foo" } }, { foo = "foo" }))
print("dict3", enc({ dict = { "x", "y", "foo" } }, { z = "x" }))
print("dict-dup", enc({ dict = { "a", "a", "b" } }, { a = 1 }))
print("dict-nested", enc({ dict = { "a", "b" } }, { a = { b = true } }))
print("dict-arr", enc({ dict = { "a" } }, { "a", "a" }))
print("dict-hole", enc({ dict = { "a", nil, "b" } }, { b = 1 }))
print("dict-many", enc({ dict = (function() local d = {} for i = 1, 300 do d[i] = "key" .. i end return d end)() }, { key300 = 1 }))

local m1, m2 = {}, {}
print("mt1", enc({ metatable = { m1 } }, setmetatable({ 1 }, m1)))
print("mt2", enc({ metatable = { m1, m2 } }, setmetatable({ 1 }, m2)))
print("mt-unknown", enc({ metatable = { m1 } }, setmetatable({ 1 }, m2)))
print("mt-none-listed", enc({}, setmetatable({ 1 }, m1)))
print("mt-named", enc({ metatable = { Foo = m1 } }, setmetatable({ 1 }, m1)))
print("mt-nested", enc({ metatable = { m1, m2 } }, { setmetatable({}, m1), setmetatable({ x = 1 }, m2) }))
print("both", enc({ dict = { "x" }, metatable = { m1 } }, setmetatable({ x = 1 }, m1)))
print("plain encode ignores mt", hex(buf.encode(setmetatable({ 1 }, m1))))

-- round trips
local x = buf.new({ dict = { "name", "age", "tags" } })
x:encode({ name = "bob", age = 30, tags = { "a", "b" } })
local r = x:decode()
print("rt dict", show(r))
local Class = { __index = { hello = function() return "hi" end } }
local y = buf.new({ metatable = { Class } })
y:encode({ setmetatable({ v = 1 }, Class), setmetatable({ v = 2 }, Class) })
local r2 = y:decode()
print("rt mt", getmetatable(r2[1]) == Class, getmetatable(r2[2]) == Class, r2[1].v, r2[2].v, r2[1]:hello())
-- the option tables are used by reference for decode: a decoder with the same dict reads the bytes
local d1 = { "alpha", "beta" }
local e1 = buf.new({ dict = d1 })
e1:encode({ alpha = 1, beta = 2 })
local bytes = tostring(e1)
local d2 = buf.new({ dict = { "alpha", "beta" } })
d2:put(bytes)
print("shared dict", show(d2:decode()))

-- decode errors
local function dec(label, opts, s)
  local b = buf.new(opts)
  b:put(s)
  P(label, b.decode, b)
end
dec("miss dict idx", { dict = { "a" } }, "\9\1\15\5\7\0\0\0\0\0\0\0\0")
dec("no dict", nil, "\9\1\15\0\8")
dec("no mts", nil, "\14\0\8")
dec("mt miss", { metatable = {} }, "\14\5\8")
dec("mt non-table next", { metatable = { m1 } }, "\14\0\0")
dec("mt ok", { metatable = { m1 } }, "\14\0\8")
-- option errors
P("opt dict nonstr", buf.new, { dict = { 1 } })
P("opt mt nontable", buf.new, { metatable = { "x" } })
P("opt dict not table", function() return type(buf.new({ dict = "abc" })) end)
