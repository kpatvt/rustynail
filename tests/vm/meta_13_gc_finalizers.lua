local order = {}
collectgarbage("stop")   -- no automatic collection while building: compare a single collection
for i = 1, 3 do setmetatable({}, {__gc = function() order[#order + 1] = i end}) end
collectgarbage()
collectgarbage("restart")
print(#order, order[1], order[2], order[3])
local resurrect
do
  local o = setmetatable({name = "R"}, {__gc = function(self) resurrect = self end})
end
collectgarbage()
print(resurrect and resurrect.name)
local count = 0
local mt = {__gc = function() count = count + 1 end}
local x = setmetatable({}, mt)
x = nil
collectgarbage() collectgarbage()
print(count)
local errobj = setmetatable({}, {__gc = function() error("in gc") end})
errobj = nil
collectgarbage()
print("survived gc error")
local nogc = setmetatable({}, {})
getmetatable(nogc).__gc = function() count = count + 100 end
nogc = nil collectgarbage()
print(count)
