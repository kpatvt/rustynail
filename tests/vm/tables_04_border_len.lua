local t = {}
for i = 1, 10 do t[i] = i end
print(#t)
t[#t + 1] = 11 t[#t + 1] = 12
print(#t)
t[12] = nil
print(#t)
t[5] = nil
print(#t == 11 or #t == 4)
local u = {1, 2, 3, nil, 5}
print(#u)
local v = {nil, nil, 3}
print(#v)
local w = {}
w[3] = 3 w[2] = 2 w[1] = 1
print(#w)
local x = {1, 2, 3}
x[#x] = nil
print(#x)
for i = 1, 100 do x[i] = i end
for i = 100, 51, -1 do x[i] = nil end
print(#x)
local y = {n = 3}
y[1], y[2], y[3] = 1, 2, 3
print(#y)
print(#"abc", #{[1] = 1, [2] = 2}, #{[2] = 2})
