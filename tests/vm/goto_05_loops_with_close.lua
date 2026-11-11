local log = {}
local function mk(n) return setmetatable({}, {__close = function() log[#log + 1] = "close" .. n end}) end
do
  local i = 0
  ::top::
  local c <close> = mk(i)
  i = i + 1
  if i < 3 then goto top end
  log[#log + 1] = "end"
end
print(table.concat and "x" or "y", #log, log[1], log[2], log[3], log[4])
for i = 1, 2 do
  local c <close> = mk("f" .. i)
  if i == 1 then goto next end
  log[#log + 1] = "body" .. i
  ::next::
end
print(#log, log[5], log[6], log[7], log[8])
