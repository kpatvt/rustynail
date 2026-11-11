local a, b, c, d = ...
local t = {x = 1}
local function f(x) return x end
local k = 1
if 0x7fffffff then a = 1 elseif d then a = 2 else a = 3 end
while 127 do a = a + 1; if (t.z ~= d) < (a * true) then break end end
repeat local q = f(a) until ~(d .. f(b)) or q
local r = (c == 1e15) < 2 < (d <= b) == (c // 255.0)
return not (1), a, r
