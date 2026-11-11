local parts = {}
local s = ""
for i = 1, 200 do s = s .. i .. "," end
print(#s, s:sub(1, 20), s:sub(-10))
local big = string.rep("x", 1000)
local r = big .. big .. big
print(#r, #(r .. r), r == string.rep("x", 3000))
local k = "k" .. 1
local t = {}
t[k] = 1
print(t.k1, t["k" .. 1], t["k1"])
local long = string.rep("a", 41)
local lk = {}
lk[long] = "v"
print(lk[string.rep("a", 41)], lk[string.rep("a", 40) .. "a"])
local a, b, c = "foo", "bar", "baz"
print(a .. b .. c .. a .. b .. c .. a .. b .. c .. a .. b .. c .. a .. b .. c .. a .. b .. c .. a .. b .. c .. a)
