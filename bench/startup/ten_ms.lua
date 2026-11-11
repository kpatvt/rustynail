-- a script of roughly 10 ms of work (typical short utility): build a table, sort, format
local t = {}
for i = 1, 2000 do t[i] = (i * 7919) % 1009 end
table.sort(t)
local parts = {}
for i = 1, 500 do parts[#parts + 1] = string.format("%d:%s", t[i], tostring(i)) end
print(#table.concat(parts, ","))
