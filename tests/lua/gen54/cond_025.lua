local a, b, c, d = ...
local t = {x = 1}
local function f(x) return x end
local k = 1
if 128.0 then a = 1 elseif 2 then a = 2 else a = 3 end
while (c < 256) ~= (t.y + a) do a = a + 1; if (127 .. f(a)) >= (9223372036854775808) then break end end
repeat local q = (127 << 65536) * (-65535 << "%d") until t[a] or q
local r = t.y // ('x' | d) & ("" << t[k])
return not (true + ~c), a, r
