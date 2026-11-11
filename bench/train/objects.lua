-- PGO training (scripts/pgo.sh): classes, inheritance, metamethods, proxies, closures,
-- varargs (shapes, an event queue, a matrix type) -- unlike the bench/programs code.
local Shape = {}; Shape.__index = Shape
function Shape.new(kind) return setmetatable({kind = kind, moves = 0}, Shape) end
function Shape:move(dx, dy) self.x = (self.x or 0) + dx; self.y = (self.y or 0) + dy; self.moves = self.moves + 1 end
function Shape:describe() return self.kind .. ":" .. self:area() end
local Rect = setmetatable({}, {__index = Shape}); Rect.__index = Rect
function Rect.new(w, h) local r = Shape.new("rect"); r.w, r.h = w, h; return setmetatable(r, Rect) end
function Rect:area() return self.w * self.h end
local Square = setmetatable({}, {__index = Rect}); Square.__index = Square
function Square.new(s) local r = Rect.new(s, s); r.kind = "square"; return setmetatable(r, Square) end
local Circle = setmetatable({}, {__index = Shape}); Circle.__index = Circle
function Circle.new(r) local c = Shape.new("circle"); c.r = r; return setmetatable(c, Circle) end
function Circle:area() return 3 * self.r * self.r end

local shapes = {}
for i = 1, 300 do
  local k = i % 3
  shapes[i] = k == 0 and Rect.new(i, 2) or k == 1 and Square.new(i % 17) or Circle.new(i % 11)
end
local total = 0
for round = 1, 400 do
  for i = 1, #shapes do
    local s = shapes[i]
    s:move(1, -1)
    total = total + s:area()
    if round % 100 == 0 and i % 50 == 0 then total = total + #s:describe() end
  end
end
print(total)

-- a 2x2 matrix type with arithmetic metamethods
local M = {}
M.__index = M
M.__mul = function(a, b)
  return setmetatable({a[1] * b[1] + a[2] * b[3], a[1] * b[2] + a[2] * b[4],
                       a[3] * b[1] + a[4] * b[3], a[3] * b[2] + a[4] * b[4]}, M)
end
M.__add = function(a, b) return setmetatable({a[1] + b[1], a[2] + b[2], a[3] + b[3], a[4] + b[4]}, M) end
M.__eq = function(a, b) return a[1] == b[1] and a[2] == b[2] and a[3] == b[3] and a[4] == b[4] end
M.__tostring = function(a) return string.format("[%d %d; %d %d]", a[1], a[2], a[3], a[4]) end
M.__len = function() return 4 end
local function mat(a, b, c, d) return setmetatable({a, b, c, d}, M) end
local acc = mat(1, 0, 0, 1)
local step = mat(1, 1, 1, 0)
local eqs = 0
for i = 1, 60000 do
  acc = acc * step
  if acc[1] > 1e9 then acc = mat(1, 0, 0, 1) end
  if acc == step then eqs = eqs + 1 end
  acc = acc + mat(0, 0, 0, 0)
  eqs = eqs + #acc - 4
end
print(tostring(acc), eqs)

-- proxies with __index/__newindex functions
local function proxy(t)
  local log = 0
  return setmetatable({}, {
    __index = function(_, k) log = log + 1; return t[k] end,
    __newindex = function(_, k, v) log = log + 1; t[k] = v end,
    __call = function() return log end,
  })
end
local p = proxy({})
for i = 1, 50000 do p[i % 97] = (p[(i + 1) % 97] or 0) + 1 end
print(p())

-- closures, upvalues, varargs, select, pcall
local function counter()
  local n = 0
  return function(...) n = n + select("#", ...); return n end
end
local c = counter()
local function sum(...)
  local s = 0
  for i = 1, select("#", ...) do s = s + (select(i, ...)) end
  return s
end
local function pack2(...) return {...}, select("#", ...) end
local acc2 = 0
for i = 1, 40000 do
  c(i, i, i)
  acc2 = acc2 + sum(i, 1, 2, 3)
  local t, n = pack2(i, nil, i)
  acc2 = acc2 + n + #t
  local ok, err = pcall(function(x) if x % 1000 == 0 then error("boom" .. x) end return x end, i)
  if not ok then acc2 = acc2 + #err end
end
print(c(), acc2)

-- an event queue with coroutines
local queue = {}
local function spawn(f, ...) queue[#queue + 1] = coroutine.create(f); return coroutine.resume(queue[#queue], ...) end
local ticks = 0
for w = 1, 50 do
  spawn(function(id)
    for k = 1, 200 do
      ticks = ticks + id % 3
      coroutine.yield(k)
    end
  end, w)
end
local alive = #queue
while alive > 0 do
  alive = 0
  for i = 1, #queue do
    local co = queue[i]
    if coroutine.status(co) == "suspended" then
      coroutine.resume(co)
      alive = alive + 1
    end
  end
end
print(ticks)
