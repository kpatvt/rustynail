-- rustynail-specific behaviour of string.buffer (documented differences from LuaJIT, docs/EXTENSIONS.md).
local C = dofile("_common.lua")
local P, hex, show = C.P, C.hex, C.show
local buf = require("string.buffer")

-- integer subtype: integral doubles within +-2^53 decode as integers, everything else as floats
local function sub(v) local r = buf.decode(buf.encode(v)) return math.type(r) .. ":" .. tostring(r) end
print("int", sub(1), sub(-5), sub(2 ^ 53), sub(1 << 53))
print("float", sub(1.5), sub(1e300), sub(-0.0), sub(1 / 0), sub(0 / 0 ~= 0 / 0 and 0.25 or 0))
print("float integral", sub(3.0), sub(2.0 ^ 40))
-- integers beyond 2^53 use the int64 tag (0x10) and keep every bit
print("bigint bytes", hex(buf.encode(math.maxinteger)), hex(buf.encode(math.mininteger)), hex(buf.encode((1 << 53) + 1)))
print("bigint rt", sub(math.maxinteger), sub(math.mininteger), sub((1 << 53) + 1), sub(-(1 << 53) - 1))
print("int tags", hex(buf.encode(1 << 53)), hex(buf.encode(-(1 << 53))))
-- decode of the other numeric tags
print("int32 tag", sub(buf.decode("\6\5\0\0\0") + 0), buf.decode("\6\255\255\255\255"))
print("uint64 tag", sub(buf.decode("\17\255\255\255\255\255\255\255\255") + 0), math.type(buf.decode("\17\1\0\0\0\0\0\0\0")))
print("int64 tag", buf.decode("\16\255\255\255\255\255\255\255\255"))
-- integer table keys and values survive
local t = buf.decode(buf.encode({ 10, 20, [100] = "x", [2 ^ 53] = true }))
print("keys", math.type(next(t)), t[1], t[100], t[1 << 53], #t)

-- light userdata (tags 3, 4, 5)
print("null", type(buf.decode("\3")), hex(buf.encode(buf.decode("\3"))))
local lu = buf.decode("\4\1\2\3\4")
print("lightud32", type(lu), hex(buf.encode(lu)))
local lu2 = buf.decode("\5\1\2\3\4\5\6\0\0")
print("lightud64", type(lu2), hex(buf.encode(lu2)))

-- cdata methods are not available without FFI
for _, m in ipairs { "reserve", "commit", "putcdata", "ref" } do
  local b = buf.new()
  local ok, err = pcall(b[m], b, 1)
  print(m, ok, (tostring(err):gsub("^[^:]*:%d+: ", ""):sub(1, 60)))
end
-- putf follows Lua 5.5 string.format: %d needs an integer representation, float prints "1.0" with %s
local b = buf.new()
P("putf %d 2.5", b.putf, b, "%d", 2.5)
b:reset() b:putf("%d %s %s", 3.0, 1.0, 10 // 3) print("putf", tostring(b))
b:reset() b:put(1.0, " ", 2, " ", 0.5) print("put numbers", tostring(b))
-- strict string arguments
P("decode number", buf.decode, 5)
P("set number", b.set, b, 5)

-- re-entrancy: a __tostring that touches the buffer being filled
local x = buf.new()
x:put("a", setmetatable({}, { __tostring = function() x:put("[inner]") return "X" end }), "b")
print("reentrant put", tostring(x))
local y = buf.new()
y:put("hello")
local mt = { __tostring = function() y:reset() return "R" end }
y:put(setmetatable({}, mt), "!")
print("reentrant reset", tostring(y))

-- many buffers: collected, memory returns
local before = collectgarbage("count")
for i = 1, 2000 do
  local z = buf.new(1000)
  z:put(("x"):rep(500))
end
collectgarbage() collectgarbage()
print("gc buffers", collectgarbage("count") - before < 200)
-- decode through gc pressure: nested tables and many strings
local big = {}
for i = 1, 300 do big[i] = { name = "n" .. i, list = { i, i + 1, i + 2 }, nested = { a = { b = "deep" .. i } } } end
local s = buf.encode(big)
local back = buf.decode(s)
local okc = 0
for i = 1, 300 do
  if back[i].name == "n" .. i and back[i].list[3] == i + 2 and back[i].nested.a.b == "deep" .. i then okc = okc + 1 end
end
print("decode big", okc, #s)
-- buffer object api on a buffer used after free / reset
local f = buf.new()
f:put("abc"):free()
print("after free", #f, f:get(), tostring(f))
-- large legitimate buffer (50 MB) and size limits without allocating
local lb = buf.new()
local chunk = ("0123456789abcdef"):rep(4096) -- 64 KB
for i = 1, 800 do lb:put(chunk) end
print("large", #lb, #lb:get(10), #lb)
lb:free()
P("size 2^31", buf.new, 2 ^ 31)
P("size 1<<62", buf.new, 1 << 62)
P("encode huge string ok", function() return #buf.encode(("x"):rep(1000000)) end)
