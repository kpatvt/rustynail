local log = {}
local t = setmetatable({}, {__newindex = function(t, k, v) log[#log + 1] = k .. "=" .. tostring(v) rawset(t, k, v) end})
t.a = 1 t.b = 2 t.a = 3
print(#log, log[1], log[2], t.a, t.b)
local store = {}
local proxy = setmetatable({}, {__newindex = store})
proxy.x = 10
print(rawget(proxy, "x"), store.x)
local chain = setmetatable({}, {__newindex = setmetatable({}, {__newindex = store})})
chain.y = 20
print(store.y, rawget(chain, "y"))
local ro = setmetatable({}, {__newindex = function() error("read-only", 2) end})
print(pcall(function() ro.x = 1 end))
local arr = setmetatable({}, {__newindex = function(t, k, v) rawset(t, k, v * 2) end})
arr[1] = 5 arr[2] = 6
print(arr[1], arr[2])
arr[1] = 100
print(arr[1])
local loop = {}
setmetatable(loop, {__newindex = loop})
print(pcall(function() loop.x = 1 end))
