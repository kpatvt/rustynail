local state, out = "a", {}
local steps = 0
::dispatch::
steps = steps + 1
if steps > 20 then goto finished end
if state == "a" then out[#out + 1] = "A" state = "b" goto dispatch
elseif state == "b" then out[#out + 1] = "B" state = "c" goto dispatch
elseif state == "c" then out[#out + 1] = "C" state = "a" goto dispatch end
::finished::
print(#out, out[1], out[2], out[3], out[#out])
