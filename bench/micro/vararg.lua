local N = 20000000
local function f(...) local a, b = ...; return a end; local x; for i = 1, N // 4 do x = f(i, 2); x = f(i, 2); x = f(i, 2); x = f(i, 2) end
print(0)
