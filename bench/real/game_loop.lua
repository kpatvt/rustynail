-- game loop: entities with OOP (metatables, inheritance), closures as behaviours, update/collide over frames
local Entity = {}
Entity.__index = Entity
function Entity.new(x, y, vx, vy) return setmetatable({ x = x, y = y, vx = vx, vy = vy, hp = 100, alive = true }, Entity) end
function Entity:update(dt)
  self.x = self.x + self.vx * dt
  self.y = self.y + self.vy * dt
  if self.x < 0 or self.x > 100 then self.vx = -self.vx end
  if self.y < 0 or self.y > 100 then self.vy = -self.vy end
end
function Entity:damage(n) self.hp = self.hp - n; if self.hp <= 0 then self.alive = false end end

local Enemy = setmetatable({}, { __index = Entity })
Enemy.__index = Enemy
function Enemy.new(x, y)
  local e = Entity.new(x, y, 1.5, -0.5)
  e.ai = 0
  return setmetatable(e, Enemy)
end
function Enemy:update(dt)
  Entity.update(self, dt)
  self.ai = self.ai + 1
  if self.ai % 30 == 0 then self.vx, self.vy = self.vy, -self.vx end
end

local function make_spawner(rate)
  local count = 0
  return function(world)
    count = count + 1
    if count % rate == 0 then world[#world + 1] = Enemy.new(count % 100, (count * 7) % 100) end
  end
end

local function make_counter()
  local n = 0
  return function(d) n = n + d; return n end
end

local world = {}
for i = 1, 100 do world[i] = (i % 2 == 0) and Enemy.new(i, i) or Entity.new(i, 100 - i, i % 5 - 2, 1) end
local spawn = make_spawner(10)
local hits = make_counter()
local score = 0
for frame = 1, 1500 do
  spawn(world)
  for i = 1, #world do world[i]:update(0.1) end
  for i = 1, #world, 3 do
    local a = world[i]
    for j = i + 1, math.min(i + 20, #world) do
      local b = world[j]
      local dx, dy = a.x - b.x, a.y - b.y
      if dx * dx + dy * dy < 4 then a:damage(5); b:damage(5); score = hits(1) end
    end
  end
  if frame % 50 == 0 then
    local k = 0
    for i = 1, #world do if world[i].alive then k = k + 1; world[k] = world[i] end end
    for i = #world, k + 1, -1 do world[i] = nil end
  end
end
print(#world, score)
