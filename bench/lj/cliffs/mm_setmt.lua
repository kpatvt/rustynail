-- N3b: `setmetatable` in hot loops, one shape per run (3M iterations).
--   lua mm_setmt.lua <new|tail|add|addeq>
--   new:   s = s + new(i, 1).x            new = function(x, y) return setmetatable({x = x, y = y}, V) end   (tail call of a C function)
--   tail:  same through a local variable (call + return of setmetatable)
--   add:   acc = acc + one                __add returns setmetatable({...}, V)  (the bench program `metamethods` shape)
--   addeq: add plus `acc == one`          (__eq Lua closure)
local kind = arg and arg[1] or "add"
local n = 3000000
local V = {}
V.__index = V
V.__add = function(a, b) return setmetatable({x = a.x + b.x, y = a.y + b.y}, V) end
V.__eq = function(a, b) return a.x == b.x and a.y == b.y end
local function new(x, y) return setmetatable({x = x, y = y}, V) end
local function mk(x, y) local o = setmetatable({x = x, y = y}, V) return o end
local function run()
  local s, acc, one = 0, new(0, 0), new(1, 2)
  if kind == "new" then for i = 1, n do s = s + new(i, 1).x end
  elseif kind == "tail" then for i = 1, n do s = s + mk(i, 1).x end
  elseif kind == "add" then for i = 1, n do acc = acc + one end s = acc.x
  else for i = 1, n do acc = acc + one if acc == one then s = s + 1 end end s = s + acc.x end
  return s
end
print(run())
