local src = {}
for i = 1, 300 do src[i] = i end
local t = {table.unpack(src, 1, 250)}
print(#t, t[1], t[250])
local u = {0, table.unpack(src, 1, 200)}
print(#u, u[1], u[2], u[201])
local v = {table.unpack(src, 1, 100), table.unpack(src, 1, 5)}
print(#v)
local function f(...) return {...} end
local w = f(table.unpack(src, 1, 240))
print(#w, w[240])
local a = {1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100, table.unpack(src, 1, 3)}
print(#a, a[100], a[101], a[103])
