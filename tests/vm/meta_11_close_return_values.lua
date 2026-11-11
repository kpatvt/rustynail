local log = {}
local function C(name) return setmetatable({}, {__close = function() log[#log + 1] = name end}) end
local function multi()
  local c <close> = C("m")
  return 1, 2, 3
end
print(multi())
print(log[1])
local function viatail()
  local c <close> = C("t")
  return multi()
end
print(viatail())
print(#log, log[2], log[3])
local function va(...)
  local c <close> = C("v")
  return ...
end
print(va(1, nil, 3))
print((va(4, 5)))
local t = {va(7, 8, 9)}
print(#t)
