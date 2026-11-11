local log = {}
local t = setmetatable({}, {
  __index = function(_, k) log[#log + 1] = "get" .. tostring(k) return "dflt" end,
  __newindex = function(_, k, v) log[#log + 1] = "set" .. tostring(k) end,
  __len = function() return 99 end,
  __eq = function() return true end,
})
local u = setmetatable({}, getmetatable(t))
print(t.x, rawget(t, "x"), #t, rawlen(t), t == u, rawequal(t, u))
t.y = 1
print(rawget(t, "y"), #log)
rawset(t, "z", 5)
print(t.z, rawget(t, "z"), #log)
print(pcall(rawset, 1, 2, 3))
print(rawequal(t, t), rawequal(1, 1.0), rawequal("a", "a"), rawequal({}, {}))
