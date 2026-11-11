local out = {}
local function P(...)
  local t = table.pack(...)
  for i = 1, t.n do t[i] = tostring(t[i]) end
  out[#out + 1] = table.concat(t, " ")
end
local DEPTH, OVF, SMALL = DEPTH_VALUE, OVF_VALUE, SMALL_VALUE
