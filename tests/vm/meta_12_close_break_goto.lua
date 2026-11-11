local log = {}
local function C(name) return setmetatable({}, {__close = function() log[#log + 1] = name end}) end
for i = 1, 3 do
  local c <close> = C("loop" .. i)
  if i == 2 then break end
end
print(#log, log[1], log[2])
log = {}
do
  local i = 0
  ::top::
  do
    local c <close> = C("g" .. i)
    i = i + 1
    if i < 3 then goto top end
  end
end
print(#log, log[1], log[3])
log = {}
local function f()
  for i = 1, 3 do
    local c <close> = C("f" .. i)
    if i == 2 then return "early" end
  end
end
print(f(), #log, log[1], log[2])
while true do
  local c <close> = C("w")
  break
end
print(#log)
