local n = tonumber(arg and arg[1]) or 60000000
local s, f = 0, 0.0
for i = 1, n do s = s + i % 7 * 3 - (i >> 2) ; f = f + i * 0.5 end
local x = 0
for i = 1, n // 2 do x = x ~ (i * 2654435761 & 0xffffffff) end
local w, k = 0, 0
while k < n // 3 do k = k + 1; if k % 3 == 0 then w = w + k else w = w - 1 end end
print(s, f, x, w)
