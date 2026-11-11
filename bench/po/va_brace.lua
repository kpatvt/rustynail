local function f(...) local t = {...} return #t end
local s = 0
for i = 1, 8000000 do s = s + f(i, i, i) end
print(s)
