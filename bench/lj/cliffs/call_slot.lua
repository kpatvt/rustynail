-- N5: callee in a call slot read from a table: Lua closures vs light C functions
-- arg: mixed | lua | c
local m = arg and arg[1] or "mixed"
local F
local function l1(x) return x + 1 end
local function l2(x) return x + 2 end
if m == "lua" then F = {l1, l2, l1, l2}
elseif m == "c" then F = {math.abs, math.floor, math.abs, math.floor}
else F = {l1, math.abs, l2, math.floor} end
local acc = 0
for i = 1, 3000000 do
  local f = F[i % 4 + 1]
  acc = acc + f(i)
end
print(acc)
