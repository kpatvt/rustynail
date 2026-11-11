local N = 20000000
local t = {}; for i = 1, 100 do t["k"..i] = i end; local s = 0; for r = 1, N // 100 do for k, v in pairs(t) do s = s + v end end
print(0)
