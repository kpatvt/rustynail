-- study micro: `{}` stored into a small ring (escapes, so not sunk); 1M iterations
local keep = {}
for i = 1, 64 do keep[i] = false end
for i = 1, 1000000 do keep[i % 64 + 1] = {} end
print(0)
