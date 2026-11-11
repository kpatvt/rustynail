-- Byte-exact serialization: encodings of values whose layout does not depend on the internals of the table
-- implementation must equal LuaJIT's bytes; decoding hand-written (LuaJIT-format) bytes must give the same values.
local C = dofile("_common.lua")
local P, hex, show = C.P, C.hex, C.show
local buf = require("string.buffer")

local function e(label, v) print(label, hex(buf.encode(v))) end
e("nil", nil) e("false", false) e("true", true)
e("0", 0) e("1", 1) e("-1", -1) e("1.5", 1.5) e("2^31", 2 ^ 31) e("2^53", 2 ^ 53) e("-0", -0.0)
e("inf", 1 / 0) e("-inf", -1 / 0) e("pi", math.pi) e("big", 1e300) e("small", 5e-324) e("int-ish", 123456789012)
e("empty str", "") e("a", "a") e("zero byte", "\0") e("embedded", "a\0b\255")
for _, n in ipairs { 30, 31, 32, 222, 223, 224, 225, 255, 256, 479, 480, 500, 8000, 8159, 8160, 8190, 10000, 70000 } do
  local s = buf.encode(("x"):rep(n))
  print("strlen", n, #s, hex(s:sub(1, 6)), hex(s:sub(-2)))
end
e("{}", {}) e("{1}", { 1 }) e("{1,2,3}", { 1, 2, 3 }) e("{x=1}", { x = 1 }) e("{1,nil,3}", { 1, nil, 3 })
e("{1,2,x=1}", { 1, 2, x = 1 }) e("{{}}", { {} }) e("{{{}}}", { { {} } })
e("{[1.5]=1}", { [1.5] = 1 }) e("{[true]=1}", { [true] = 1 }) e("{[-1]=1}", { [-1] = 1 }) e("{[100]=1}", { [100] = 1 })
e("{[true]=false}", { [true] = false }) e("{1,nil,nil,4}", { 1, nil, nil, 4 })
e("nested", { a = { b = { c = 1 } } })
e("str keys 1", { name = "value" })
e("{[0]}", (function() local t = {} t[0] = 7 return t end)())
e("{[0],1}", (function() local t = { 1 } t[0] = 7 return t end)())
e("assign arr", (function() local t = {} t[1] = 1 t[2] = 2 t[3] = 3 return t end)())
e("trail nil", (function() local t = { 1, 2, 3 } t[3] = nil return t end)())
e("cleared", (function() local t = {} for i = 1, 10 do t[i] = i end for i = 1, 10 do t[i] = nil end return t end)())
print("mixed", show(buf.decode(buf.encode((function() local t = { x = 1 } t[1] = 5 return t end)()))))
e("ctor idx", { n = 1, [1] = 1, [2] = 2 })
e("arr 300", (function() local t = {} for i = 1, 300 do t[i] = true end return t end)())
e("arr strs", { "a", "bb", "ccc" })
e("arr nested", { { 1 }, { 2, 3 }, { x = 4 } })
e("arr bools", { true, false, true })
for _, n in ipairs { 222, 223, 224, 225, 500 } do
  local t = {} for i = 1, n do t[i] = true end
  local s = buf.encode(t)
  print("arrlen", n, hex(s:sub(1, 4)))
end

-- decode of hand-written bytes (the wire format, see docs/EXTENSIONS.md)
local function d(label, bytes)
  local s = bytes:gsub("%s", ""):gsub("%x%x", function(h) return string.char(tonumber(h, 16)) end)
  P(label, buf.decode, s)
end
d("d nil", "00") d("d false", "01") d("d true", "02")
d("d num", "07 00 00 00 00 00 00 f0 3f") d("d num frac", "07 00 00 00 00 00 00 f8 3f")
d("d neg", "07 00 00 00 00 00 00 f0 bf") d("d int32", "06 05 00 00 00") d("d int32 neg", "06 ff ff ff ff")
d("d str", "23 61 62 63") d("d empty str", "20") d("d str2", "e0 00" .. (" 78"):rep(224))
d("d tab", "08") d("d arr", "0c 03 07 00 00 00 00 00 00 f0 3f 07 00 00 00 00 00 00 00 40")
d("d arr0", "0a 02 07 00 00 00 00 00 00 f0 3f 07 00 00 00 00 00 00 00 40")
d("d hash", "09 01 21 78 07 00 00 00 00 00 00 f0 3f")
d("d arr+hash", "0d 02 01 07 00 00 00 00 00 00 f0 3f 21 78 21 79")
d("d arr0+hash", "0b 01 01 07 00 00 00 00 00 00 f0 3f 21 78 21 79")
d("d nested", "0c 02 0c 02 0c 02 08") d("d holes", "0c 04 07 00 00 00 00 00 00 f0 3f 00 07 00 00 00 00 00 00 08 40")
d("d tab-keys", "09 01 08 21 76") d("d bool key", "09 01 02 21 76")

-- malformed input: errors, never a crash
d("e empty", "") d("e trunc num", "07 00 00") d("e trunc str", "25 61") d("e trunc tab", "0c 02") d("e trunc hash", "09 01 21 78")
d("e tag 13", "13") d("e tag 3", "03") d("e tag 4", "04 00 00 00 00") d("e tag 0x14", "14") d("e tag 0x1f", "1f")
d("e leftover", "00 01") d("e nil key", "09 01 00 00") d("e nan key", "09 01 07 00 00 00 00 00 00 f8 ff 00")
d("e dict no dict", "09 01 0f 00 08") d("e mt no mts", "0e 00 08")
d("e huge arr", "0c ff ff ff ff 7f") d("e huge hash", "09 ff ff ff ff 7f") d("e huge str", "ff ff ff ff 7f")
d("e str len trunc", "e0 ff")
for n = 1, 19 do
  -- every proper prefix of a valid encoding is an error (or, for a few short prefixes, a valid shorter value)
  local full = buf.encode({ 1, "ab", x = { true } })
  local pre = full:sub(1, n)
  local ok, err = pcall(buf.decode, pre)
  print("prefix", n, ok, ok and "" or C.norm(err))
end
-- depth limit
local function nest(n) local t = {} local p = t for i = 1, n do p[1] = {} p = p[1] end return t end
print("depth ok", pcall(buf.encode, nest(99)))
P("depth 100", buf.encode, nest(100))
P("depth 101", buf.encode, nest(101))
local deep = ("0c 02 "):rep(100):gsub("%s", ""):gsub("%x%x", function(h) return string.char(tonumber(h, 16)) end) .. "\0"
print("decode depth 100", pcall(buf.decode, deep))
local deeper = ("\12\2"):rep(101) .. "\0"
P("decode depth 101", buf.decode, deeper)
local c = {} c.c = c
P("circular", buf.encode, c)
-- unsupported types
P("function", buf.encode, print)
P("thread", buf.encode, coroutine.create(print))
P("userdata", buf.encode, io.stdout)
P("fn in table", buf.encode, { 1, print })
P("fn key", buf.encode, { [print] = 1 })
local keep = buf.new() keep:put("pre")
pcall(keep.encode, keep, { 1, print })
print("rollback", #keep, tostring(keep))
-- encode appends; decode consumes one value at a time
local x = buf.new()
x:encode("hi"):encode(5):encode({ 1, 2 }):encode(nil):encode(false)
print("stream len", #x)
for i = 1, 5 do print("stream", i, show(x:decode())) end
print("stream left", #x)
P("stream eob", x.decode, x)
