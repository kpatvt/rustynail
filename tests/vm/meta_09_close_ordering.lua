local log = {}
local function C(name) return setmetatable({}, {__close = function(_, e) log[#log + 1] = name .. ":" .. tostring(e) end}) end
do
  local a <close> = C("a")
  local b <close> = C("b")
  local c <close> = C("c")
  log[#log + 1] = "body"
end
print(table.concat and 0 or #log, log[1], log[2], log[3], log[4])
log = {}
local function f()
  local x <close> = C("x")
  local y <close> = C("y")
  return "result"
end
print(f(), log[1], log[2])
log = {}
print(pcall(function()
  local p <close> = C("p")
  local q <close> = C("q")
  error("oops", 0)
end))
print(log[1], log[2])
log = {}
local nilc <close> = nil
local falsec <close> = false
print("nil/false ok")
print(pcall(function() local bad <close> = 42 end))
print(pcall(function() local bad <close> = {} end))
