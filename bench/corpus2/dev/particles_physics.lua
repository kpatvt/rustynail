-- 2D Verlet particle physics with spring constraints and a spatial hash; vec math via
-- small helper functions on plain tables. small: N=2
local N = tonumber(arg and arg[1]) or 75

local seed = 777
local function frand()
  seed = (seed * 16807) % 2147483647
  return (seed % 100000) / 100000
end

local sqrt, floor, abs = math.sqrt, math.floor, math.abs

-- vector helpers -----------------------------------------------------------
local function vec(x, y) return { x = x, y = y } end
local function add(a, b) return vec(a.x + b.x, a.y + b.y) end
local function sub(a, b) return vec(a.x - b.x, a.y - b.y) end
local function scale(a, s) return vec(a.x * s, a.y * s) end
local function dot(a, b) return a.x * b.x + a.y * b.y end
local function length(a) return sqrt(dot(a, a)) end
local function normalize(a)
  local l = length(a)
  if l < 1e-9 then return vec(0, 0) end
  return scale(a, 1 / l)
end

-- particles ----------------------------------------------------------------
local particles, springs = {}, {}
local W, H = 200, 120
local GRAVITY = vec(0, 30)
local RADIUS = 2.5

local function newParticle(x, y, pinned)
  local p = { pos = vec(x, y), prev = vec(x, y), acc = vec(0, 0), pinned = pinned, id = #particles + 1 }
  particles[#particles + 1] = p
  return p
end

local function connect(a, b, stiffness)
  springs[#springs + 1] = { a = a, b = b, rest = length(sub(a.pos, b.pos)), k = stiffness or 0.5 }
end

-- build a cloth grid and some loose blobs
local COLS = 10 + (N > 10 and 6 or 0)
local ROWS = 8 + (N > 10 and 4 or 0)
local grid = {}
for r = 1, ROWS do
  grid[r] = {}
  for c = 1, COLS do
    local p = newParticle(20 + c * 8, 10 + r * 6, r == 1 and (c % 3 == 1))
    grid[r][c] = p
    if c > 1 then connect(grid[r][c - 1], p, 0.8) end
    if r > 1 then connect(grid[r - 1][c], p, 0.8) end
  end
end
for i = 1, 25 + N do
  newParticle(60 + frand() * 100, 5 + frand() * 20, false)
end

-- spatial hash ---------------------------------------------------------------
local Hash = {}
Hash.__index = Hash
function Hash.new(cell) return setmetatable({ cell = cell, buckets = {} }, Hash) end
function Hash:key(x, y)
  return floor(x / self.cell) * 4096 + floor(y / self.cell)
end
function Hash:insert(p)
  local k = self:key(p.pos.x, p.pos.y)
  local b = self.buckets[k]
  if b then b[#b + 1] = p else self.buckets[k] = { p } end
end
function Hash:neighbors(p, fn)
  local cx, cy = floor(p.pos.x / self.cell), floor(p.pos.y / self.cell)
  for dx = -1, 1 do
    for dy = -1, 1 do
      local b = self.buckets[(cx + dx) * 4096 + (cy + dy)]
      if b then
        for i = 1, #b do
          if b[i].id > p.id then fn(b[i]) end
        end
      end
    end
  end
end

-- simulation steps -----------------------------------------------------------
local function integrate(dt)
  for _, p in ipairs(particles) do
    if not p.pinned then
      local vel = sub(p.pos, p.prev)
      vel = scale(vel, 0.995)
      p.prev = p.pos
      p.pos = add(add(p.pos, vel), scale(GRAVITY, dt * dt))
    end
  end
end

local function satisfySprings()
  for _, s in ipairs(springs) do
    local d = sub(s.b.pos, s.a.pos)
    local len = length(d)
    if len > 1e-9 then
      local diff = (len - s.rest) / len * 0.5 * s.k
      local corr = scale(d, diff)
      if not s.a.pinned then s.a.pos = add(s.a.pos, corr) end
      if not s.b.pinned then s.b.pos = sub(s.b.pos, corr) end
    end
  end
end

local collisions = 0
local function collide()
  local hash = Hash.new(RADIUS * 2)
  for _, p in ipairs(particles) do hash:insert(p) end
  for _, p in ipairs(particles) do
    hash:neighbors(p, function(q)
      local d = sub(q.pos, p.pos)
      local dist = length(d)
      if dist < RADIUS * 2 and dist > 1e-9 then
        collisions = collisions + 1
        local push = scale(normalize(d), (RADIUS * 2 - dist) * 0.5)
        if not p.pinned then p.pos = sub(p.pos, push) end
        if not q.pinned then q.pos = add(q.pos, push) end
      end
    end)
  end
end

local function bounds()
  for _, p in ipairs(particles) do
    if not p.pinned then
      if p.pos.y > H then p.pos.y = H; p.prev.y = p.pos.y + (p.pos.y - p.prev.y) * 0.3 end
      if p.pos.x < 0 then p.pos.x = 0 end
      if p.pos.x > W then p.pos.x = W end
    end
  end
end

local function energy()
  local e = 0
  for _, p in ipairs(particles) do
    local v = sub(p.pos, p.prev)
    e = e + 0.5 * dot(v, v)
  end
  return e
end

local function stretch()
  local worst, total = 0, 0
  for _, s in ipairs(springs) do
    local l = length(sub(s.a.pos, s.b.pos))
    local e = abs(l - s.rest) / s.rest
    total = total + e
    if e > worst then worst = e end
  end
  return total / #springs, worst
end

local steps = N * 4
local dt = 1 / 30
for step = 1, steps do
  integrate(dt)
  for _ = 1, 4 do satisfySprings() end
  collide()
  bounds()
  if step % math.max(1, floor(steps / 6)) == 0 then
    local avg, worst = stretch()
    print(string.format("step %4d energy=%9.5f avgStretch=%.5f worst=%.5f", step, energy(), avg, worst))
  end
end

local cx, cy = 0, 0
for _, p in ipairs(particles) do cx = cx + p.pos.x; cy = cy + p.pos.y end
print(string.format("particles=%d springs=%d collisions=%d", #particles, #springs, collisions))
print(string.format("centre of mass %.4f %.4f", cx / #particles, cy / #particles))
for _, i in ipairs({ 1, 5, COLS, #particles }) do
  local p = particles[i]
  print(string.format("p%-3d %9.4f %9.4f", p.id, p.pos.x, p.pos.y))
end
