-- Entity-component-system game simulation: component stores, systems, spawn/despawn,
-- fixed-timestep loop and uniform-grid collision. small: N=2
local N = tonumber(arg and arg[1]) or 400

local seed = 12345
local function rand(n)
  seed = (seed * 16807) % 2147483647
  return seed % n
end

---------------------------------------------------------------------------
-- World: entities are integer ids; each component type has its own store
---------------------------------------------------------------------------
local World = {}
World.__index = World

function World.new()
  local self = setmetatable({}, World)
  self.nextId = 1
  self.alive = {}
  self.stores = {}
  self.freeList = {}
  self.count = 0
  return self
end

function World:store(name)
  local s = self.stores[name]
  if not s then
    s = {}
    self.stores[name] = s
  end
  return s
end

function World:spawn(components)
  local id = table.remove(self.freeList)
  if not id then
    id = self.nextId
    self.nextId = id + 1
  end
  self.alive[id] = true
  self.count = self.count + 1
  for name, value in pairs(components) do
    self:store(name)[id] = value
  end
  return id
end

function World:despawn(id)
  if not self.alive[id] then return end
  self.alive[id] = nil
  self.count = self.count - 1
  for _, s in pairs(self.stores) do
    s[id] = nil
  end
  self.freeList[#self.freeList + 1] = id
end

-- Return a sorted list of entity ids having all of the given components.
function World:query(...)
  local names = { ... }
  local first = self:store(names[1])
  local result = {}
  for id in pairs(first) do
    local ok = true
    for i = 2, #names do
      if self.stores[names[i]][id] == nil then ok = false; break end
    end
    if ok then result[#result + 1] = id end
  end
  table.sort(result)
  return result
end

---------------------------------------------------------------------------
-- Systems
---------------------------------------------------------------------------
local WORLD_W, WORLD_H = 1000, 1000
local CELL = 50

local function movementSystem(world, dt)
  local pos, vel = world.stores.pos, world.stores.vel
  for _, id in ipairs(world:query("pos", "vel")) do
    local p, v = pos[id], vel[id]
    p.x = p.x + v.x * dt
    p.y = p.y + v.y * dt
    if p.x < 0 then p.x = -p.x; v.x = -v.x end
    if p.x > WORLD_W then p.x = 2 * WORLD_W - p.x; v.x = -v.x end
    if p.y < 0 then p.y = -p.y; v.y = -v.y end
    if p.y > WORLD_H then p.y = 2 * WORLD_H - p.y; v.y = -v.y end
  end
end

local function lifetimeSystem(world, dt)
  local life = world.stores.life
  local dead = {}
  for _, id in ipairs(world:query("life")) do
    life[id].t = life[id].t - dt
    if life[id].t <= 0 then dead[#dead + 1] = id end
  end
  for _, id in ipairs(dead) do world:despawn(id) end
  return #dead
end

local function collisionSystem(world, stats)
  local pos, body = world.stores.pos, world.stores.body
  local grid = {}
  local ids = world:query("pos", "body")
  for _, id in ipairs(ids) do
    local p = pos[id]
    local cx, cy = math.floor(p.x / CELL), math.floor(p.y / CELL)
    local key = cy * 1000 + cx
    local bucket = grid[key]
    if not bucket then bucket = {}; grid[key] = bucket end
    bucket[#bucket + 1] = id
  end
  local hits = {}
  for _, id in ipairs(ids) do
    local p, r = pos[id], body[id].r
    local cx, cy = math.floor(p.x / CELL), math.floor(p.y / CELL)
    for gy = cy - 1, cy + 1 do
      for gx = cx - 1, cx + 1 do
        local bucket = grid[gy * 1000 + gx]
        if bucket then
          for _, other in ipairs(bucket) do
            if other > id then
              local q = pos[other]
              local dx, dy = q.x - p.x, q.y - p.y
              local rr = r + body[other].r
              stats.tests = stats.tests + 1
              if dx * dx + dy * dy < rr * rr then
                hits[#hits + 1] = { id, other }
              end
            end
          end
        end
      end
    end
  end
  return hits
end

local function damageSystem(world, hits)
  local health = world.stores.health
  local killed = 0
  for _, pair in ipairs(hits) do
    for _, id in ipairs(pair) do
      local h = health[id]
      if h then
        h.hp = h.hp - 1
        if h.hp <= 0 and world.alive[id] then
          world:despawn(id)
          killed = killed + 1
        end
      end
    end
  end
  return killed
end

---------------------------------------------------------------------------
-- Spawning
---------------------------------------------------------------------------
local function spawnShip(world, kind)
  return world:spawn {
    pos = { x = rand(WORLD_W), y = rand(WORLD_H) },
    vel = { x = rand(61) - 30, y = rand(61) - 30 },
    body = { r = 4 + rand(5) },
    health = { hp = 3 + rand(4) },
    tag = kind,
  }
end

local function spawnBullet(world)
  return world:spawn {
    pos = { x = rand(WORLD_W), y = rand(WORLD_H) },
    vel = { x = rand(201) - 100, y = rand(201) - 100 },
    body = { r = 2 },
    life = { t = 1 + rand(30) / 10 },
  }
end

---------------------------------------------------------------------------
-- Fixed timestep loop
---------------------------------------------------------------------------
local world = World.new()
local stats = { tests = 0, hits = 0, killed = 0, expired = 0, spawned = 0 }
for i = 1, 40 + N do spawnShip(world, i % 3 == 0 and "enemy" or "friend"); stats.spawned = stats.spawned + 1 end

local DT = 1 / 30
local accumulator = 0
local frameTime = 1 / 20
local steps, frames = 0, N * 2
for frame = 1, frames do
  accumulator = accumulator + frameTime
  while accumulator >= DT do
    accumulator = accumulator - DT
    steps = steps + 1
    movementSystem(world, DT)
    stats.expired = stats.expired + lifetimeSystem(world, DT)
    local hits = collisionSystem(world, stats)
    stats.hits = stats.hits + #hits
    stats.killed = stats.killed + damageSystem(world, hits)
  end
  for _ = 1, 3 + rand(5) do spawnBullet(world); stats.spawned = stats.spawned + 1 end
  if frame % 4 == 0 and world.count < 60 + N then
    spawnShip(world, "friend"); stats.spawned = stats.spawned + 1
  end
  if frame % math.max(1, math.floor(frames / 5)) == 0 then
    print(string.format("frame %4d: alive=%4d steps=%5d hits=%6d", frame, world.count, steps, stats.hits))
  end
end

---------------------------------------------------------------------------
-- Report
---------------------------------------------------------------------------
local tags = {}
for _, id in ipairs(world:query("tag")) do
  local t = world.stores.tag[id]
  tags[t] = (tags[t] or 0) + 1
end
local names = {}
for k in pairs(tags) do names[#names + 1] = k end
table.sort(names)
for _, k in ipairs(names) do print(string.format("tag %-8s %d", k, tags[k])) end

local sx, sy = 0, 0
for _, id in ipairs(world:query("pos")) do
  sx = sx + world.stores.pos[id].x
  sy = sy + world.stores.pos[id].y
end
print(string.format("spawned=%d killed=%d expired=%d tests=%d", stats.spawned, stats.killed, stats.expired, stats.tests))
print(string.format("alive=%d nextId=%d free=%d", world.count, world.nextId, #world.freeList))
print(string.format("centroid sum %.3f %.3f", sx, sy))
