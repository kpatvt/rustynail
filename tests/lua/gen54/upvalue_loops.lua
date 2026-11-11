
local fs = {}
for i = 1, 3 do
  local x = i
  fs[#fs+1] = function() x = x + 1; return x + i end
end
local j = 0
while j < 3 do
  local y = j
  fs[#fs+1] = function() return y end
  j = j + 1
end
repeat
  local z = j
  fs[#fs+1] = function() return z end
until z > 2
for k, v in pairs(fs) do local w = k; fs[k] = function() return w, v end end
return fs
