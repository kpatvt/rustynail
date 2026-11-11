-- Derived from rxi/classic (https://github.com/rxi/classic), Copyright (c) 2014, rxi, MIT License; see THIRD_PARTY.md. Object:extend() with __call constructors + a vector/matrix class using arithmetic metamethods
-- (hump.vector / brinevector style), workload: a particle system with springs.
local Object = {}
Object.__index = Object

function Object:new() end

function Object:extend()
  local cls = {}
  for k, v in pairs(self) do
    if k:find("__") == 1 then
      cls[k] = v
    end
  end
  cls.__index = cls
  cls.super = self
  setmetatable(cls, self)
  return cls
end

function Object:implement(...)
  for _, cls in pairs({...}) do
    for k, v in pairs(cls) do
      if self[k] == nil and type(v) == "function" then
        self[k] = v
      end
    end
  end
end

function Object:is(T)
  local mt = getmetatable(self)
  while mt do
    if mt == T then
      return true
    end
    mt = getmetatable(mt)
  end
  return false
end

function Object:__tostring()
  return "Object"
end

function Object:__call(...)
  local obj = setmetatable({}, self)
  obj:new(...)
  return obj
end

-- Vector class
local Vec = Object:extend()
function Vec:new(x, y) self.x = x or 0; self.y = y or 0 end
function Vec.__add(a, b) return Vec(a.x + b.x, a.y + b.y) end
function Vec.__sub(a, b) return Vec(a.x - b.x, a.y - b.y) end
function Vec.__mul(a, b)
  if type(a) == "number" then return Vec(a * b.x, a * b.y)
  elseif type(b) == "number" then return Vec(a.x * b, a.y * b)
  else return a.x * b.x + a.y * b.y end
end
function Vec.__div(a, b) return Vec(a.x / b, a.y / b) end
function Vec.__unm(a) return Vec(-a.x, -a.y) end
function Vec.__eq(a, b) return a.x == b.x and a.y == b.y end
function Vec.__lt(a, b) return a.x < b.x or (a.x == b.x and a.y < b.y) end
function Vec.__le(a, b) return a.x <= b.x and a.y <= b.y end
function Vec.len(a) return math.sqrt(a.x * a.x + a.y * a.y) end
function Vec:__tostring() return string.format("(%.3f, %.3f)", self.x, self.y) end
function Vec:len2() return self.x * self.x + self.y * self.y end
function Vec:normalized()
  local l = self:len()
  if l > 0 then return self / l end
  return Vec(0, 0)
end
function Vec:clone() return Vec(self.x, self.y) end

local Particle = Object:extend()
function Particle:new(pos, mass)
  self.pos = pos
  self.vel = Vec(0, 0)
  self.force = Vec(0, 0)
  self.mass = mass
end
function Particle:applyForce(f) self.force = self.force + f end
function Particle:integrate(dt)
  local acc = self.force / self.mass
  self.vel = (self.vel + acc * dt) * 0.995
  self.pos = self.pos + self.vel * dt
  self.force = Vec(0, 0)
end

local HeavyParticle = Particle:extend()
function HeavyParticle:new(pos, mass)
  HeavyParticle.super.new(self, pos, mass * 4)
end
function HeavyParticle:__tostring() return "Heavy" .. tostring(self.pos) end

local Spring = Object:extend()
function Spring:new(a, b, k)
  self.a, self.b, self.k = a, b, k
  self.rest = (b.pos - a.pos):len()
end
function Spring:apply()
  local d = self.b.pos - self.a.pos
  local len = d:len()
  if len == 0 then return end
  local f = d:normalized() * ((len - self.rest) * self.k)
  self.a:applyForce(f)
  self.b:applyForce(-f)
end

local N = tonumber(arg and arg[1]) or 1000
local particles, springs = {}, {}
local W = 12
for y = 1, W do
  for x = 1, W do
    local P = ((x + y) % 5 == 0) and HeavyParticle or Particle
    particles[#particles + 1] = P(Vec(x * 10, y * 10), 1)
  end
end
for y = 1, W do
  for x = 1, W do
    local i = (y - 1) * W + x
    if x < W then springs[#springs + 1] = Spring(particles[i], particles[i + 1], 0.8) end
    if y < W then springs[#springs + 1] = Spring(particles[i], particles[i + W], 0.8) end
  end
end
local gravity = Vec(0, 0.5)
local ordered = 0
for step = 1, N do
  for _, s in ipairs(springs) do s:apply() end
  for i, p in ipairs(particles) do
    if i > W then
      p:applyForce(gravity * p.mass)
      p:integrate(0.1)
    end
  end
  if particles[1].pos < particles[2].pos then ordered = ordered + 1 end
  if particles[5].pos == particles[5].pos:clone() then ordered = ordered + 1 end
end
local c = Vec(0, 0)
for _, p in ipairs(particles) do c = c + p.pos end
c = c / #particles
local heavy = 0
for _, p in ipairs(particles) do if p:is(HeavyParticle) then heavy = heavy + 1 end end
print("centroid", tostring(c), "ordered", ordered, "heavy", heavy)
print(tostring(particles[#particles]), tostring(particles[W + 5]))
