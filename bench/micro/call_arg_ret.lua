local N = 20000000
local function f(x) return x end; local a; for i = 1, N do a = f(i); a = f(i); a = f(i); a = f(i) end
print(0)
