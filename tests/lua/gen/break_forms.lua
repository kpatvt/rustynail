
local t = {...}
while true do break end
repeat if t[1] then break end until false
for i = 1, 3 do for j = 1, 3 do if i == j then break end end if i then break end end
for k in pairs(t) do local f = function() return k end; if f then break end end
return t
