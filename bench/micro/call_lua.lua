local N = 20000000
local function f() end; for i = 1, N do f(); f(); f(); f() end
print(0)
