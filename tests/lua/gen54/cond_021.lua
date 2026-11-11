local a, b, c, d = ...
local t = {x = 1}
local function f(x) return x end
local k = 1
if ~"hello" > (127 ^ 0x7fffffff) ^ ~-127 then a = 1 elseif (1.0 | 128.0) > a then a = 2 else a = 3 end
while not -1/0 do a = a + 1; if (127.0 >= c) | d then break end end
repeat local q = "a" ^ -127 until 0.5 > false or q
local r = -(nil or 131072) | (c == t.y)
return not ((65535 and 255) and -1000000007), a, r
