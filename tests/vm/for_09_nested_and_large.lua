local s = 0
for i = 1, 100 do for j = 1, 100 do s = s + i * j end end
print(s)
local c = 0
for i = 1, 1000000 do c = c + 1 end
print(c)
local t = {}
for i = 1, 1000 do t[i] = i end
local sum = 0
for _, v in ipairs(t) do sum = sum + v end
print(sum)
sum = 0
for k, v in pairs(t) do sum = sum + k end
print(sum)
local f = 0.0
for i = 1, 1000 do f = f + 0.5 end
print(f)
