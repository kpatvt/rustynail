local a, b = 1, 2.5
local s = "hello" .. 'world' .. [[long
string]]
local function add(x, y) return x + y end
local t = {1, 2, 3, x = 10, [20] = "v", f = add}
for i = 1, 10, 2 do a = a + i end
for k, v in pairs(t) do b = b + (type(v) == "number" and v or 0) end
while a > 0 do a = a - 100 end
repeat b = b - 1 until b < 0
goto done
::done::
print(add(a, b), s, #t, -a, ~7, 1 // 2, 2 ^ 10, 0xff, 1e100, 0x1p4)
return t
