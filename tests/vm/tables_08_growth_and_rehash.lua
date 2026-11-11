local t = {}
for i = 1, 1000 do t[i] = i end
for i = 1, 1000, 2 do t[i] = nil end
local c = 0
for k in pairs(t) do c = c + 1 end
print(c, t[2], t[1000], t[999])
local u = {}
for i = 1, 100 do u["k" .. i] = i end
for i = 1, 100, 2 do u["k" .. i] = nil end
c = 0
for k, v in pairs(u) do c = c + v end
print(c)
local w = {}
for i = 1000, 1, -1 do w[i] = i end
print(#w, w[1], w[1000])
local mixed = {}
for i = 1, 50 do mixed[i] = i mixed["s" .. i] = i end
c = 0
for k, v in pairs(mixed) do c = c + 1 end
print(c, #mixed)
