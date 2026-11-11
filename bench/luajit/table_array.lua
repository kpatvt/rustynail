local floor = math.floor
local unpack = unpack or table.unpack
local n = tonumber(arg and arg[1]) or 300000
local t = {}
for i = 1, n do t[#t + 1] = (i * 7919) % 100003 end
local s = 0
for i = 1, #t do s = s + t[i] end
for r = 1, 5 do for i, v in ipairs(t) do s = s + v end end
local u = {}
for i = 1, floor(n / 4) do table.insert(u, i) end
for i = 1, floor(n / 8) do s = s + table.remove(u) end
for i = 1, 2000 do table.insert(u, 1, i); s = s + table.remove(u, 1) end
table.sort(t)
local m = {}
for i = 1, floor(n / 5) do m[i] = (i * 31337) % 65537 end
table.sort(m, function(a, b) return a > b end)
print(string.format("%d", s), string.format("%d", t[1]), string.format("%d", t[#t]), string.format("%d", m[1]), string.format("%d", m[#m]), #u, select('#', unpack(t, 1, 200)))
