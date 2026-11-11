-- C stack overflow through nested pcall / metamethods / callbacks: the depth reached must be identical
local function first_depth(start)
  return function(f)
    local depth = 0
    local ok, e = pcall(f, function() depth = depth + 1 end)
    return depth, ok, (tostring(e):gsub("%d+", "N"):gsub("0x%x+", "A"))
  end
end
local run = first_depth()
-- nested pcall
P("pcall", run(function(tick)
  local function f() tick() local ok, e = pcall(f) if not ok then error(e, 0) end return 1 end
  return f()
end))
-- nested pcall without rethrow: depth at which pcall itself starts failing
do
  local depth, last = 0, nil
  local function f() depth = depth + 1 local ok, e = pcall(f) if not ok then last = e end return 1 end
  f()
  P("pcall2", depth, (tostring(last):gsub("%d+", "N")))
end
-- __index function chain
P("index", run(function(tick)
  local t
  t = setmetatable({}, {__index = function(_, k) tick() return t[k + 1] end})
  return t[1]
end))
-- __call chain
if OVF then  -- (Lua stack overflow: 300k frames, too slow under stackmove)
P("call", run(function(tick)
  local t
  t = setmetatable({}, {__call = function(self, n) tick() local r = self(n + 1) return r end})
  return t(1)
end))
end
-- __add chain
P("add", run(function(tick)
  local mt = {}
  local a = setmetatable({}, mt)
  mt.__add = function(x, y) tick() return x + y end
  return a + a
end))
-- __concat / __eq / __lt / __len
P("concat", run(function(tick)
  local mt = {}
  local a = setmetatable({}, mt)
  mt.__concat = function(x, y) tick() return x .. y end
  return a .. a
end))
P("len", run(function(tick)
  local mt = {}
  local a = setmetatable({}, mt)
  mt.__len = function(x) tick() return #x end
  return #a
end))
-- callbacks from C functions
P("gsub", run(function(tick)
  local function f(s) tick() return (s:gsub(".", f)) end
  return f("abc")
end))
P("sort", run(function(tick)
  local function deeper() tick() table.sort({2, 1}, function(a, b) deeper() return a < b end) end
  deeper()
end))
P("tostring", run(function(tick)
  local mt = {}
  mt.__tostring = function(x) tick() local r = tostring(x) return r end
  return tostring(setmetatable({}, mt))
end))
-- nested coroutines
P("resume", run(function(tick)
  local function f() tick() local co = coroutine.wrap(f) return co() end
  return f()
end))
-- close handlers
P("close", run(function(tick)
  local budget = 3000
  local function f()
    tick()
    local x <close> = setmetatable({}, {__close = function() budget = budget - 1 if budget > 0 then f() end end})
  end
  f()
end))
-- recovery afterwards: ordinary calls still work in a hot loop
local s = 0
for i = 1, 100 do s = s + select("#", pcall(function() return i, i end)) end
P("after", s)
return table.concat(out, "\n")
