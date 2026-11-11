-- finalizers of old objects (and of objects created while closing) run when the state closes
local log = {}
x = setmetatable({}, {__gc = function() print("x finalized at close") end})
y = setmetatable({}, {__gc = function() print("y finalized at close") end})
collectgarbage()           -- x and y become old in generational mode
collectgarbage("step", 0)
z = setmetatable({}, {__gc = function() print("z finalized at close") end})
print("end of script")
