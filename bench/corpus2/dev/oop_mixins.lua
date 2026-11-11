-- Class library with single inheritance via setmetatable/__index chains, super calls,
-- mixins, tostring/eq/lt metamethods; shapes and accounting domains. small: N=2
local N = tonumber(arg and arg[1]) or 80000

-- Tiny class library ---------------------------------------------------------------------
local function class(name, parent)
  local c = { name = name, super = parent, __index = nil }
  c.__index = c
  if parent then
    setmetatable(c, { __index = parent, __call = function(cls, ...) return cls.new(...) end })
    -- metamethods are looked up with rawget, so copy them down
    for _, mm in ipairs({ "__tostring", "__eq", "__lt", "__le", "__add" }) do
      c[mm] = parent[mm]
    end
  else
    setmetatable(c, { __call = function(cls, ...) return cls.new(...) end })
  end
  c.new = function(...)
    local obj = setmetatable({}, c)
    obj:init(...)
    return obj
  end
  return c
end

local function include(cls, mixin)
  for k, v in pairs(mixin) do
    if k ~= "included" then cls[k] = v end
  end
  if mixin.included then mixin.included(cls) end
end

local function isInstance(obj, cls)
  local c = getmetatable(obj)
  while c do
    if c == cls then return true end
    c = c.super
  end
  return false
end

-- Mixins ------------------------------------------------------------------------------------
local Comparable = {
  __lt = function(a, b) return a:area() < b:area() end,
  __le = function(a, b) return a:area() <= b:area() end,
}

local Describable = {
  describe = function(self)
    return string.format("%s#%d(%s)", self.class_name, self.id, self:summary())
  end,
}

local counter = 0
local function nextId() counter = counter + 1; return counter end

-- Shapes: Shape -> Polygon -> Rect -> Square, Shape -> Circle -----------------------------
local Shape = class("Shape")
function Shape:init(x, y)
  self.id = nextId()
  self.x, self.y = x, y
  self.class_name = "Shape"
end
function Shape:area() return 0 end
function Shape:perimeter() return 0 end
function Shape:summary() return string.format("at %d,%d", self.x, self.y) end
function Shape:move(dx, dy) self.x = self.x + dx; self.y = self.y + dy; return self end
Shape.__tostring = function(s) return s:describe() end
Shape.__eq = function(a, b) return a:area() == b:area() and a:perimeter() == b:perimeter() end
include(Shape, Comparable)
include(Shape, Describable)

local Polygon = class("Polygon", Shape)
function Polygon:init(x, y, sides)
  Shape.init(self, x, y)
  self.sides = sides
  self.class_name = "Polygon"
end
function Polygon:summary() return Shape.summary(self) .. " sides=" .. self.sides end

local Rect = class("Rect", Polygon)
function Rect:init(x, y, w, h)
  Polygon.init(self, x, y, 4)
  self.w, self.h = w, h
  self.class_name = "Rect"
end
function Rect:area() return self.w * self.h end
function Rect:perimeter() return 2 * (self.w + self.h) end

local Square = class("Square", Rect)
function Square:init(x, y, s)
  Rect.init(self, x, y, s, s)
  self.class_name = "Square"
end
function Square:summary() return Rect.summary(self) .. " square" end

local Circle = class("Circle", Shape)
function Circle:init(x, y, r)
  Shape.init(self, x, y)
  self.r = r
  self.class_name = "Circle"
end
function Circle:area() return 3 * self.r * self.r end
function Circle:perimeter() return 6 * self.r end

-- Accounting: Account -> Savings / Checking, with Auditable mixin -----------------------------
local Auditable = {
  included = function(cls) cls.audited = true end,
  audit = function(self, what, amount)
    self.history[#self.history + 1] = what .. ":" .. amount
    if #self.history > 5 then table.remove(self.history, 1) end
  end,
}

local Account = class("Account")
function Account:init(owner, balance)
  self.id = nextId()
  self.owner = owner
  self.balance = balance or 0
  self.history = {}
end
function Account:deposit(n) self.balance = self.balance + n; self:audit("dep", n); return self end
function Account:withdraw(n)
  if n > self:available() then self:audit("deny", n); return false end
  self.balance = self.balance - n
  self:audit("wd", n)
  return true
end
function Account:available() return self.balance end
function Account:monthEnd() end
Account.__tostring = function(a) return a.owner .. ":" .. a.balance end
Account.__lt = function(a, b) return a.balance < b.balance end
Account.__eq = function(a, b) return a.owner == b.owner end
include(Account, Auditable)

local Savings = class("Savings", Account)
function Savings:init(owner, balance, rate)
  Account.init(self, owner, balance)
  self.rate = rate
end
function Savings:monthEnd()
  local interest = math.floor(self.balance * self.rate / 1000)
  self:deposit(interest)
end

local Checking = class("Checking", Account)
function Checking:init(owner, balance, overdraft)
  Account.init(self, owner, balance)
  self.overdraft = overdraft
end
function Checking:available() return Account.available(self) + self.overdraft end
function Checking:monthEnd()
  if self.balance < 0 then self.balance = self.balance - 25; self:audit("fee", 25) end
end

local Premium = class("Premium", Savings)
function Premium:init(owner, balance, rate)
  Savings.init(self, owner, balance, rate * 2)
end
function Premium:monthEnd()
  Savings.monthEnd(self)
  self:deposit(10)
end

-- Workload -------------------------------------------------------------------------------------------
local seed = 1010
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

local shapes = {}
for i = 1, N do
  local k = rnd(4)
  local x, y = rnd(100), rnd(100)
  if k == 1 then shapes[i] = Rect(x, y, rnd(10), rnd(10))
  elseif k == 2 then shapes[i] = Square(x, y, rnd(10))
  elseif k == 3 then shapes[i] = Circle(x, y, rnd(6))
  else shapes[i] = Polygon.new(x, y, 3 + rnd(5)) end
end

local totalArea, totalPerim, byClass = 0, 0, {}
for round = 1, 5 do
  for _, s in ipairs(shapes) do
    totalArea = totalArea + s:area()
    totalPerim = totalPerim + s:perimeter()
    s:move(1, -1)
  end
end
for _, s in ipairs(shapes) do byClass[s.class_name] = (byClass[s.class_name] or 0) + 1 end
local names = {}
for k in pairs(byClass) do names[#names + 1] = k end
table.sort(names)
for _, k in ipairs(names) do print(string.format("%-8s %5d", k, byClass[k])) end
print(string.format("area=%d perimeter=%d", totalArea, totalPerim))

local biggest, equalPairs = shapes[1], 0
for i = 2, #shapes do
  if biggest < shapes[i] then biggest = shapes[i] end
  if i % 2 == 0 and shapes[i] == shapes[i - 1] then equalPairs = equalPairs + 1 end
end
print("biggest: " .. tostring(biggest))
print("equal neighbours: " .. equalPairs)
print("sample: " .. tostring(shapes[1]) .. " | " .. tostring(shapes[2]))
print(tostring(isInstance(shapes[1], Shape)), tostring(isInstance(Square(0, 0, 1), Rect)), tostring(isInstance(Circle(0, 0, 1), Rect)))

table.sort(shapes, function(a, b)
  if a < b then return true end
  if b < a then return false end
  return a.id < b.id
end)
print("smallest: " .. tostring(shapes[1]) .. "  largest: " .. tostring(shapes[#shapes]))

local accounts = {}
for i = 1, math.max(3, math.floor(N / 4)) do
  local k = rnd(4)
  local owner = "u" .. i
  if k == 1 then accounts[i] = Savings(owner, rnd(1000), 5)
  elseif k == 2 then accounts[i] = Checking(owner, rnd(500), 200)
  elseif k == 3 then accounts[i] = Premium(owner, rnd(2000), 4)
  else accounts[i] = Account(owner, rnd(100)) end
end
local denied = 0
for month = 1, 12 do
  for _, a in ipairs(accounts) do
    if not a:withdraw(rnd(300)) then denied = denied + 1 end
    a:deposit(rnd(250))
    a:monthEnd()
  end
end
local sum = 0
for _, a in ipairs(accounts) do sum = sum + a.balance end
table.sort(accounts, function(a, b) if a < b then return true elseif b < a then return false end return a.id < b.id end)
print(string.format("accounts=%d denied=%d total=%d audited=%s", #accounts, denied, sum, tostring(Premium.audited)))
print("poorest " .. tostring(accounts[1]) .. " richest " .. tostring(accounts[#accounts]) ..
  " hist=" .. table.concat(accounts[#accounts].history, ","))
