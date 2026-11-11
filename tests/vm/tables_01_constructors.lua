local t = {1, 2, 3, x = 1, y = 2, [10] = "ten", [1.5] = "f", [true] = "b", ["z z"] = 1}
print(#t, t.x, t.y, t[10], t[1.5], t[true], t["z z"])
local function mr() return 1, 2, 3 end
local u = {mr(), mr(), n = mr()}
print(#u, u.n)
local v = {[1] = "a", "b"}
print(v[1], #v)
local w = {n = 1, n = 2}
print(w.n)
print(#{}, #{nil}, #{nil, nil}, #{1, nil}, #{n = 1})
local big = {1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55}
print(#big, big[1], big[50], big[51], big[55])
local nested = {{1, 2}, {3, {4, 5}}, x = {y = {z = "deep"}}}
print(nested[1][2], nested[2][2][1], nested.x.y.z)
