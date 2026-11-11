local t = {}
for i = 1, 5 do t[i] = i end
t.a = "a" t.b = "b"
local arr = {}
for k, v in pairs(t) do arr[#arr + 1] = tostring(k) end
print(arr[1], arr[2], arr[3], arr[4], arr[5], #arr)
local k, v = next(t)
print(k, v)
print(next(t, 5) ~= nil, next({}, nil))
print(pcall(next, {}, "nokey"))
local e = {}
print(next(e), rawlen(e))
local u = {10, 20, 30}
local c = 0
for k, v in next, u do c = c + k end
print(c)
local m = {x = 1}
m.x = nil
print(next(m))
