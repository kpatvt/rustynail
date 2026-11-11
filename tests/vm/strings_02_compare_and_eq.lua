local a, b = "abc", "ab" .. "c"
print(a == b, a ~= b, rawequal(a, b), a < "abd", a <= b, a >= b, "a" < "B", "" == "")
local long1, long2 = string.rep("z", 100), string.rep("z", 99) .. "z"
print(long1 == long2, long1 ~= long2, long1 < long2 .. "a", rawequal(long1, long2))
local t = {[long1] = 1}
print(t[long2], t[string.rep("z", 100)])
print("a\0b" == "a\0b", "a\0b" == "a\0c", #"a\0b", ("a\0b"):byte(2))
print("\65\066\x43\u{48}\z
       I", "tab\tnl\\n", 'single"', "dbl'")
print([[long
string]], [==[with ]] inside]==], #[[

x]])
