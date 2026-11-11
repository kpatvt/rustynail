-- raytrace: small recursive ray tracer (spheres and a checkered plane, point lights with shadows, reflections to
-- depth 3), vectors as small tables with metatable methods (allocation-heavy, floating point).  Renders a fixed
-- scene at a small resolution several times.  Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
-- Output: sum of the 8-bit RGB values of the image and the number of rays traced.
local ROUNDS = tonumber(arg and arg[1]) or 2
local W, H = 96, 72
local sqrt, floor, max, min = math.sqrt, math.floor, math.max, math.min

local V = {}
V.__index = V
local function vec(x, y, z) return setmetatable({ x = x, y = y, z = z }, V) end
function V:add(o) return vec(self.x + o.x, self.y + o.y, self.z + o.z) end
function V:sub(o) return vec(self.x - o.x, self.y - o.y, self.z - o.z) end
function V:mul(k) return vec(self.x * k, self.y * k, self.z * k) end
function V:dot(o) return self.x * o.x + self.y * o.y + self.z * o.z end
function V:norm()
  local l = sqrt(self:dot(self))
  return vec(self.x / l, self.y / l, self.z / l)
end

local Sphere = {}
Sphere.__index = Sphere
function Sphere:hit(o, d)
  local oc = o:sub(self.c)
  local b = oc:dot(d)
  local c = oc:dot(oc) - self.r * self.r
  local disc = b * b - c
  if disc < 0 then return nil end
  local s = sqrt(disc)
  local t = -b - s
  if t > 1e-4 then return t end
  t = -b + s
  if t > 1e-4 then return t end
  return nil
end
function Sphere:normal(p) return p:sub(self.c):mul(1 / self.r) end
function Sphere:color(p) return self.col end

local Plane = {}
Plane.__index = Plane
function Plane:hit(o, d)
  if d.y > -1e-6 and d.y < 1e-6 then return nil end
  local t = (self.y - o.y) / d.y
  if t > 1e-4 then return t end
  return nil
end
function Plane:normal(p) return vec(0, 1, 0) end
function Plane:color(p)
  local cx, cz = floor(p.x), floor(p.z)
  if (cx + cz) % 2 == 0 then return vec(0.9, 0.9, 0.9) end
  return vec(0.2, 0.2, 0.25)
end

local objs = {
  setmetatable({ c = vec(0, 1, 6), r = 1, col = vec(0.9, 0.2, 0.2), refl = 0.3 }, Sphere),
  setmetatable({ c = vec(-2.2, 0.7, 5), r = 0.7, col = vec(0.2, 0.9, 0.3), refl = 0.2 }, Sphere),
  setmetatable({ c = vec(2, 0.5, 4.5), r = 0.5, col = vec(0.2, 0.3, 0.95), refl = 0.5 }, Sphere),
  setmetatable({ y = 0, refl = 0.25 }, Plane),
}
local lights = { vec(-5, 8, -2), vec(6, 5, 0) }
local rays = 0

local function trace(o, d, depth)
  rays = rays + 1
  local best, bestT = nil, 1e30
  for i = 1, #objs do
    local t = objs[i]:hit(o, d)
    if t and t < bestT then best, bestT = objs[i], t end
  end
  if not best then
    local k = 0.5 * (d.y + 1)
    return vec(0.6 + 0.2 * k, 0.7 + 0.2 * k, 1.0)
  end
  local p = o:add(d:mul(bestT))
  local n = best:normal(p)
  local base = best:color(p)
  local shade = 0.15
  for li = 1, #lights do
    local ldir = lights[li]:sub(p)
    local dist = sqrt(ldir:dot(ldir))
    ldir = ldir:mul(1 / dist)
    local lit = true
    for i = 1, #objs do
      local t = objs[i]:hit(p, ldir)
      if t and t < dist then lit = false break end
    end
    if lit then shade = shade + 0.6 * max(0, n:dot(ldir)) / #lights end
  end
  local col = base:mul(shade)
  if depth < 3 and best.refl > 0 then
    local r = d:sub(n:mul(2 * d:dot(n)))
    col = col:mul(1 - best.refl):add(trace(p, r, depth + 1):mul(best.refl))
  end
  return col
end

local function render()
  local sum = 0
  local eye = vec(0, 1.5, -3)
  for y = 0, H - 1 do
    for x = 0, W - 1 do
      local dx = (x - W / 2) / H * 1.4
      local dy = (H / 2 - y) / H * 1.4 - 0.1
      local d = vec(dx, dy, 1):norm()
      local c = trace(eye, d, 0)
      sum = sum + floor(min(1, c.x) * 255) + floor(min(1, c.y) * 255) + floor(min(1, c.z) * 255)
    end
  end
  return sum
end

local total = 0
for r = 1, ROUNDS do total = render() end
print("raytrace", total, rays)
