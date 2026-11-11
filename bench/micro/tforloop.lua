local N = 20000000
local t = {}; for i = 1, 100 do t[i] = i end; local s = 0; for r = 1, N // 100 do for i, v in ipairs(t) do s = s + v end end
print(0)
