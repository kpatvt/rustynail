-- Bounce: balls bouncing in a box (port from the public are-we-fast-yet "Bounce" description):
-- 100 balls, 50 steps each per iteration; verification: the iteration counts 1331 bounces.
-- Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
local N = tonumber(arg and arg[1]) or 350
local abs = math.abs

local Random = {}
Random.__index = Random
local function newRandom() return setmetatable({ seed = 74755 }, Random) end
function Random:next()
  self.seed = (self.seed * 1309 + 13849) % 65536
  return self.seed
end

local Ball = {}
Ball.__index = Ball
local function newBall(random)
  return setmetatable({ x = random:next() % 500, y = random:next() % 500,
    xVel = (random:next() % 300) - 150, yVel = (random:next() % 300) - 150 }, Ball)
end
function Ball:bounce()
  local xLimit, yLimit = 500, 500
  local bounced = false
  self.x = self.x + self.xVel
  self.y = self.y + self.yVel
  if self.x > xLimit then self.x = xLimit; self.xVel = 0 - abs(self.xVel); bounced = true end
  if self.x < 0 then self.x = 0; self.xVel = abs(self.xVel); bounced = true end
  if self.y > yLimit then self.y = yLimit; self.yVel = 0 - abs(self.yVel); bounced = true end
  if self.y < 0 then self.y = 0; self.yVel = abs(self.yVel); bounced = true end
  return bounced
end

local total, ok = 0, 0
for it = 1, N do
  local random = newRandom()
  local balls = {}
  for i = 1, 100 do balls[i] = newBall(random) end
  local bounces = 0
  for i = 1, 50 do
    for j = 1, 100 do
      if balls[j]:bounce() then bounces = bounces + 1 end
    end
  end
  total = total + bounces
  if bounces == 1331 then ok = ok + 1 end
end
print("bounce", ok, total)
