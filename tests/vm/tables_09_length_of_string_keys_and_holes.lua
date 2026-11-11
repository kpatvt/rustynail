local t = {10, 20, 30, nil}
print(#t)
local u = {nil}
print(#u)
local w = {1, nil, 3}
print(#w)
local x = {n = 1, 1, 2}
print(#x)
local y = {}
y[1] = 1 y[2] = 2 y[4] = 4
print(#y)
y[3] = 3
print(#y)
local z = {1, 2, 3}
z[5] = 5
print(#z)
z[4] = 4
print(#z)
