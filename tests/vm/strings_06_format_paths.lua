-- string.format paths: plain `%d`/`%i` without snprintf, conversions appended straight into the
-- result buffer (including outputs longer than the initial reserve), literal runs, `%s` with and
-- without modifiers or `__tostring`, `%p` of non-pointers, `%c`, `%q`.
local F = string.format
for _, v in ipairs({0, 7, -7, 42, -100, 123456789, math.maxinteger, math.mininteger, 3.0, -2^53}) do
  print(F("%d|%i|%5d|%-5d|%05d|%+d|% d|%x|%X|%o|%.3d", v, v, v, v, v, v, v, v, v, v, v))
end
print(pcall(F, "%d", 1.5), pcall(F, "%d", "x"), pcall(F, "%d"))
print(F("%d", "10"), F("%i", " 0x10 "))
-- long conversions (growth past the first reserve), several in a row
print(#F("%.99f", 1e300), F("%.99f", 1 / 3):sub(1, 20), #F("%99.99f|%99.99f", 2, -2))
print(F("%5.2f|%-8.3e|%g|%G|%a", 1 / 3, 12345.678, 1e20, 1e-20, 1.0))
-- literal runs and %%
print(F("abc%%def%%%d%%ghi", 5), F("no specs at all"), F(""), F("%%"))
print(F(string.rep("x", 300) .. "%d" .. string.rep("y", 300), 1):len())
-- %s
local t = setmetatable({}, {__tostring = function() return "T!" end})
local u = setmetatable({}, {__name = "MyType"})
print(F("[%s][%s][%s][%s][%s]", "str", 12, 1.5, true, nil), F("%s", t), F("%10s|%-10s|", t, "ab"))
print(F("%s", u):match("^MyType: ") ~= nil, F("%.2s", "abcdef"), F("%5.1s|", "xyz"))
print(#F("%s", string.rep("z", 1000)), #F("%-5s", string.rep("z", 150)))
print(pcall(F, "%10s", "a\0b"), F("%s", "a\0b") == "a\0b")
local mt = getmetatable("")
mt.__tostring = function(s) return "<" .. s .. ">" end
print(F("%s|%5s", "hi", "yo"))
mt.__tostring = nil
-- %c, %p, %q
print(F("%c%c%c", 76, 117, 97), F("%5c|%-5c|", 65, 66))
print(F("%p", 1), F("%p", nil), F("%10p|", true), F("%p", {}) ~= F("%p", {}))
print(F("%q", "a\nb\"c\0d\1" .. "2"), F("%q", 1 / 0), F("%q", math.mininteger), F("%q", 0.1))
print(pcall(F, "%10q", "x"), pcall(F, "%w", 1), pcall(F, "%123d", 1), pcall(F, "%", 1))
