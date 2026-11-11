local a, b = ...
local r = {}
r[#r+1] = 1.0 < nil
r[#r+1] = 256 < 255
r[#r+1] = 256 < 1.0
r[#r+1] = 255 < 0
r[#r+1] = 0 < b
r[#r+1] = 1 < a
if math.huge < -1 then r[#r+1] = 1 end
r[#r+1] = b <= true
r[#r+1] = 255 <= nil
r[#r+1] = 256 <= 256
r[#r+1] = 's' <= a
r[#r+1] = 127.0 <= 1e100
r[#r+1] = 1.0 <= 127
if b <= math.huge then r[#r+1] = 1 end
r[#r+1] = 1.0 > b
r[#r+1] = 127.0 > -128
r[#r+1] = 256 > 1
r[#r+1] = a > math.huge
r[#r+1] = 1 > 1.0
r[#r+1] = 255 > 1e100
if 128.0 > true then r[#r+1] = 1 end
r[#r+1] = 0.5 >= 's'
r[#r+1] = b >= 1.0
r[#r+1] = math.huge >= math.huge
r[#r+1] = -1 >= 127.0
r[#r+1] = nil >= math.huge
r[#r+1] = -1 >= -1
if 0.5 >= 0.5 then r[#r+1] = 1 end
r[#r+1] = -128 == 1.0
r[#r+1] = 127.0 == 127.0
r[#r+1] = 1e100 == 128
r[#r+1] = 128.0 == math.huge
r[#r+1] = 256 == math.huge
r[#r+1] = 127 == -128
if 127.0 == true then r[#r+1] = 1 end
r[#r+1] = -128 ~= 128.0
r[#r+1] = 's' ~= 1
r[#r+1] = math.huge ~= nil
r[#r+1] = 255 ~= nil
r[#r+1] = 0 ~= -1
r[#r+1] = nil ~= b
if 128 ~= 1 then r[#r+1] = 1 end
return r
