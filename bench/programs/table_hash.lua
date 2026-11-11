local n = tonumber(arg and arg[1]) or 200000
local t = {}
for i = 1, n do t["key" .. i] = i end
local s = 0
for r = 1, 5 do for i = 1, n do s = s + t["key" .. i] end end
local c = 0
for k, v in pairs(t) do c = c + v end
for i = 1, n, 2 do t["key" .. i] = nil end
local d = 0
for k in pairs(t) do d = d + 1 end
local h = {}
for i = 1, n * 4 do h[i * 7919 % 1000003] = i end
local e = 0
for i = 1, n * 4 do e = e + (h[i * 7919 % 1000003] or 0) end
print(s, c, d, e)
