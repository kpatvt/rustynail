local log = {}
local function iter()
  local closer = setmetatable({}, {__close = function() log[#log + 1] = "closed" end})
  local i = 0
  return function() i = i + 1 if i <= 3 then return i end end, nil, nil, closer
end
for i in iter() do log[#log + 1] = "it" .. i end
print(#log, log[1], log[4])
log = {}
for i in iter() do if i == 2 then break end end
print(#log, log[1])
log = {}
print(pcall(function() for i in iter() do error("boom", 0) end end))
print(#log, log[1])
log = {}
local function f() for i in iter() do return "ret" .. i end end
print(f(), #log, log[1])
