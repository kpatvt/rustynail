-- Derived from middleclass (https://github.com/kikito/middleclass), Copyright (c) Enrique García Cota, MIT License; see THIRD_PARTY.md. Plus a simulation workload (inheritance, super calls, mixins).
local middleclass = {}

local function _createIndexWrapper(aClass, f)
  if f == nil then
    return aClass.__instanceDict
  elseif type(f) == "function" then
    return function(self, name)
      local value = aClass.__instanceDict[name]
      if value ~= nil then
        return value
      else
        return (f(self, name))
      end
    end
  else -- if  type(f) == "table" then
    return function(self, name)
      local value = aClass.__instanceDict[name]
      if value ~= nil then
        return value
      else
        return f[name]
      end
    end
  end
end

local function _propagateInstanceMethod(aClass, name, f)
  f = name == "__index" and _createIndexWrapper(aClass, f) or f
  aClass.__instanceDict[name] = f
  for subclass in pairs(aClass.subclasses) do
    if rawget(subclass.__declaredMethods, name) == nil then
      _propagateInstanceMethod(subclass, name, f)
    end
  end
end

local function _declareInstanceMethod(aClass, name, f)
  aClass.__declaredMethods[name] = f
  if f == nil and aClass.super then
    f = aClass.super.__instanceDict[name]
  end
  _propagateInstanceMethod(aClass, name, f)
end

local function _tostring(self) return "class " .. self.name end
local function _call(self, ...) return self:new(...) end

local function _createClass(name, super)
  local dict = {}
  dict.__index = dict

  local aClass = { name = name, super = super, static = {},
                   __instanceDict = dict, __declaredMethods = {},
                   subclasses = setmetatable({}, {__mode='k'})  }

  if super then
    setmetatable(aClass.static, {
      __index = function(_,k)
        local result = rawget(dict,k)
        if result == nil then
          return super.static[k]
        end
        return result
      end
    })
  else
    setmetatable(aClass.static, { __index = function(_,k) return rawget(dict,k) end })
  end

  setmetatable(aClass, { __index = aClass.static, __tostring = _tostring,
                         __call = _call, __newindex = _declareInstanceMethod })

  return aClass
end

local function _includeMixin(aClass, mixin)
  assert(type(mixin) == 'table', "mixin must be a table")

  for name,method in pairs(mixin) do
    if name ~= "included" and name ~= "static" then aClass[name] = method end
  end

  for name,method in pairs(mixin.static or {}) do
    aClass.static[name] = method
  end

  if type(mixin.included)=="function" then mixin:included(aClass) end
  return aClass
end

local DefaultMixin = {
  __tostring   = function(self) return "instance of " .. tostring(self.class) end,

  initialize   = function(self, ...) end,

  isInstanceOf = function(self, aClass)
    return type(aClass) == 'table'
       and type(self) == 'table'
       and (self.class == aClass
            or type(self.class) == 'table'
            and type(self.class.isSubclassOf) == 'function'
            and self.class:isSubclassOf(aClass))
  end,

  static = {
    allocate = function(self)
      assert(type(self) == 'table', "Make sure that you are using 'Class:allocate' instead of 'Class.allocate'")
      return setmetatable({ class = self }, self.__instanceDict)
    end,

    new = function(self, ...)
      assert(type(self) == 'table', "Make sure that you are using 'Class:new' instead of 'Class.new'")
      local instance = self:allocate()
      instance:initialize(...)
      return instance
    end,

    subclass = function(self, name)
      assert(type(self) == 'table', "Make sure that you are using 'Class:subclass' instead of 'Class.subclass'")
      assert(type(name) == "string", "You must provide a name(string) for your class")

      local subclass = _createClass(name, self)

      for methodName, f in pairs(self.__instanceDict) do
        if not (methodName == "__index" and type(f) == "table") then
          _propagateInstanceMethod(subclass, methodName, f)
        end
      end
      subclass.initialize = function(instance, ...) return self.initialize(instance, ...) end

      self.subclasses[subclass] = true
      self:subclassed(subclass)

      return subclass
    end,

    subclassed = function(self, other) end,

    isSubclassOf = function(self, other)
      return type(other)      == 'table' and
             type(self.super) == 'table' and
             ( self.super == other or self.super:isSubclassOf(other) )
    end,

    include = function(self, ...)
      assert(type(self) == 'table', "Make sure you that you are using 'Class:include' instead of 'Class.include'")
      for _,mixin in ipairs({...}) do _includeMixin(self, mixin) end
      return self
    end
  }
}

function middleclass.class(name, super)
  assert(type(name) == 'string', "A name (string) is needed for the new class")
  return super and super:subclass(name) or _includeMixin(_createClass(name), DefaultMixin)
end

setmetatable(middleclass, { __call = function(_, ...) return middleclass.class(...) end })
local class = middleclass

-------------------------------------------------------------------------------
-- Workload: an entity simulation
-------------------------------------------------------------------------------
local HasWings = {
  fly = function(self, dt) self.y = self.y + self.lift * dt; self.energy = self.energy - dt * 0.5 end,
}
local Serializable = {
  serialize = function(self)
    return string.format("%s(%d,%d,%d)", self.class.name, math.floor(self.x), math.floor(self.y), math.floor(self.energy))
  end,
}

local Entity = class('Entity')
Entity:include(Serializable)
function Entity:initialize(x, y)
  self.x, self.y = x, y
  self.energy = 100
  self.alive = true
end
function Entity:update(dt)
  self.energy = self.energy - dt
  if self.energy <= 0 then self.alive = false end
end
function Entity:distance2(other)
  local dx, dy = self.x - other.x, self.y - other.y
  return dx * dx + dy * dy
end
function Entity.__eq(a, b) return a.x == b.x and a.y == b.y end
function Entity.__lt(a, b) return a.energy < b.energy end

local Animal = class('Animal', Entity)
function Animal:initialize(x, y, speed)
  Entity.initialize(self, x, y)
  self.speed = speed
  self.vx, self.vy = speed, -speed * 0.5
end
function Animal:update(dt)
  Entity.update(self, dt * 0.25)
  self.x = self.x + self.vx * dt
  self.y = self.y + self.vy * dt
  if self.x < 0 or self.x > 1000 then self.vx = -self.vx end
  if self.y < 0 or self.y > 1000 then self.vy = -self.vy end
end
function Animal:speak() return "..." end

local Dog = class('Dog', Animal)
function Dog:speak() return "woof" end
function Dog:update(dt)
  Animal.update(self, dt)
  self.energy = self.energy + 0.01
end

local Bird = class('Bird', Animal)
Bird:include(HasWings)
function Bird:initialize(x, y, speed)
  Animal.initialize(self, x, y, speed)
  self.lift = 2
end
function Bird:speak() return "tweet" end
function Bird:update(dt)
  Animal.update(self, dt)
  self:fly(dt)
end

local Parrot = class('Parrot', Bird)
function Parrot:speak() return "hello " .. Bird.speak(self) end

-- __index fallback on a class (middleclass feature)
local Config = class('Config')
Config.__index = function(self, k) return "default_" .. k end

local N = tonumber(arg and arg[1]) or 5000

local seed = 42
local function rnd(n) seed = (seed * 16807) % 2147483647; return seed % n end

local world = {}
local kinds = { Dog, Bird, Parrot, Animal }
for i = 1, 300 do
  local K = kinds[1 + rnd(#kinds)]
  world[i] = K:new(rnd(1000), rnd(1000), 1 + rnd(5))
end

local speaks, near, inst = 0, 0, 0
for step = 1, N do
  for i = 1, #world do
    local e = world[i]
    e:update(0.1)
    if #e:speak() > 4 then speaks = speaks + 1 end
  end
  -- sparse pairwise checks
  for i = 1, #world, 7 do
    local a = world[i]
    for j = i + 1, math.min(#world, i + 20) do
      if a:distance2(world[j]) < 2500 then near = near + 1 end
      if a < world[j] then inst = inst + 1 end
    end
  end
  if step % 10 == 0 then
    for i = 1, #world do
      if world[i]:isInstanceOf(Bird) then inst = inst + 1 end
    end
    local c = Config:new()
    if c.missing == "default_missing" then inst = inst + 1 end
  end
end
local alive = 0
for i = 1, #world do if world[i].alive then alive = alive + 1 end end
print("speaks", speaks, "near", near, "inst", inst, "alive", alive)
print(world[1]:serialize(), world[2]:serialize(), tostring(Parrot), Parrot:isSubclassOf(Entity))
print(tostring(world[3]):sub(1, 20))
