local i = 0
while i < 5 do i = i + 1 if i == 3 then goto c end io.write(i, " ") ::c:: end print()
local n = 0
repeat local k = n n = n + 1 until k >= 3
print(n)
local fs = {}
local m = 0
repeat local v = m fs[#fs + 1] = function() return v end m = m + 1 until v >= 2
print(fs[1](), fs[3]())
while true do break end
repeat until true
local x = 10
while x > 0 do x = x - 3 end
print(x)
do local a = 1 while a < 100 do a = a * 2 end print(a) end
