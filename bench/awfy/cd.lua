-- CD: aircraft collision detection (port from the public description of the are-we-fast-yet "CD" benchmark,
-- itself derived from the real-time Java "CDx" benchmark): each frame the new aircraft positions are reduced to
-- the 2D voxels their motion crosses (a red-black tree maps voxel -> motions), and motions sharing a voxel are
-- tested for a collision (closest-approach test of two moving spheres).  Allocation- and tree-heavy, double
-- arithmetic, many tiny method calls.  Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
-- Output: number of collisions found and number of voxel groups examined (the result is independent of the engine).
local N = tonumber(arg and arg[1]) or 1
local NUM_AIRCRAFT = 100
local NUM_FRAMES = 100
local PROXIMITY_RADIUS = 1.0
local VOXEL_SIZE = 10.0
local sqrt, cos, sin, floor = math.sqrt, math.cos, math.sin, math.floor
local insert = table.insert
local HUGE = 1e300

-- Vector2D / Vector3D / CallSign ------------------------------------------------------------------------
local Vector2D = {}
Vector2D.__index = Vector2D
local function newV2(x, y) return setmetatable({ x = x, y = y }, Vector2D) end
function Vector2D:compareTo(o)
  if self.x < o.x then return -1 end
  if self.x > o.x then return 1 end
  if self.y < o.y then return -1 end
  if self.y > o.y then return 1 end
  return 0
end
function Vector2D:plus(o) return newV2(self.x + o.x, self.y + o.y) end

local Vector3D = {}
Vector3D.__index = Vector3D
local function newV3(x, y, z) return setmetatable({ x = x, y = y, z = z }, Vector3D) end
function Vector3D:plus(o) return newV3(self.x + o.x, self.y + o.y, self.z + o.z) end
function Vector3D:minus(o) return newV3(self.x - o.x, self.y - o.y, self.z - o.z) end
function Vector3D:dot(o) return self.x * o.x + self.y * o.y + self.z * o.z end
function Vector3D:times(a) return newV3(self.x * a, self.y * a, self.z * a) end
function Vector3D:squaredMagnitude() return self:dot(self) end

local CallSign = {}
CallSign.__index = CallSign
local function newCallSign(v) return setmetatable({ value = v }, CallSign) end
function CallSign:compareTo(o)
  if self.value < o.value then return -1 end
  if self.value > o.value then return 1 end
  return 0
end

-- Red-black tree (left-leaning), keys have :compareTo -------------------------------------------------
local RBT = {}
RBT.__index = RBT
local function newRBT() return setmetatable({ root = nil, size = 0 }, RBT) end
local function isRed(h) return h ~= nil and h.red end
local function rotateLeft(h)
  local x = h.right
  h.right = x.left
  x.left = h
  x.red = h.red
  h.red = true
  return x
end
local function rotateRight(h)
  local x = h.left
  h.left = x.right
  x.right = h
  x.red = h.red
  h.red = true
  return x
end
local function flipColors(h)
  h.red = not h.red
  h.left.red = not h.left.red
  h.right.red = not h.right.red
end
local function balance(h)
  if isRed(h.right) then h = rotateLeft(h) end
  if isRed(h.left) and isRed(h.left.left) then h = rotateRight(h) end
  if isRed(h.left) and isRed(h.right) then flipColors(h) end
  return h
end
local function moveRedLeft(h)
  flipColors(h)
  if isRed(h.right.left) then
    h.right = rotateRight(h.right)
    h = rotateLeft(h)
    flipColors(h)
  end
  return h
end
local function moveRedRight(h)
  flipColors(h)
  if isRed(h.left.left) then
    h = rotateRight(h)
    flipColors(h)
  end
  return h
end
local oldValue
local function put(tree, h, key, val)
  if h == nil then
    tree.size = tree.size + 1
    oldValue = nil
    return { key = key, value = val, red = true }
  end
  local cmp = key:compareTo(h.key)
  if cmp < 0 then
    h.left = put(tree, h.left, key, val)
  elseif cmp > 0 then
    h.right = put(tree, h.right, key, val)
  else
    oldValue = h.value
    h.value = val
  end
  if isRed(h.right) and not isRed(h.left) then h = rotateLeft(h) end
  if isRed(h.left) and isRed(h.left.left) then h = rotateRight(h) end
  if isRed(h.left) and isRed(h.right) then flipColors(h) end
  return h
end
function RBT:put(key, val)
  self.root = put(self, self.root, key, val)
  self.root.red = false
  return oldValue
end
function RBT:get(key)
  local x = self.root
  while x ~= nil do
    local cmp = key:compareTo(x.key)
    if cmp < 0 then x = x.left elseif cmp > 0 then x = x.right else return x.value end
  end
  return nil
end
local function minNode(h)
  while h.left ~= nil do h = h.left end
  return h
end
local function deleteMin(h)
  if h.left == nil then return nil end
  if not isRed(h.left) and not isRed(h.left.left) then h = moveRedLeft(h) end
  h.left = deleteMin(h.left)
  return balance(h)
end
local function delete(h, key)
  if key:compareTo(h.key) < 0 then
    if not isRed(h.left) and not isRed(h.left.left) then h = moveRedLeft(h) end
    h.left = delete(h.left, key)
  else
    if isRed(h.left) then h = rotateRight(h) end
    if key:compareTo(h.key) == 0 and h.right == nil then return nil end
    if not isRed(h.right) and not isRed(h.right.left) then h = moveRedRight(h) end
    if key:compareTo(h.key) == 0 then
      local x = minNode(h.right)
      h.key = x.key
      h.value = x.value
      h.right = deleteMin(h.right)
    else
      h.right = delete(h.right, key)
    end
  end
  return balance(h)
end
function RBT:remove(key)
  if self:get(key) == nil then return nil end
  local v = self:get(key)
  if not isRed(self.root.left) and not isRed(self.root.right) then self.root.red = true end
  self.root = delete(self.root, key)
  if self.root ~= nil then self.root.red = false end
  self.size = self.size - 1
  return v
end
local function forEach(h, f)
  if h == nil then return end
  forEach(h.left, f)
  f(h.key, h.value)
  forEach(h.right, f)
end
function RBT:forEach(f) forEach(self.root, f) end

-- Motion ---------------------------------------------------------------------------------------------------
local Motion = {}
Motion.__index = Motion
local function newMotion(callsign, posOne, posTwo)
  return setmetatable({ callsign = callsign, posOne = posOne, posTwo = posTwo }, Motion)
end
function Motion:delta() return self.posTwo:minus(self.posOne) end
-- closest approach of two spheres moving linearly during t in [0,1]; returns the collision point or nil
function Motion:findIntersection(other)
  local v = self.posOne:minus(other.posOne)
  local w = self:delta():minus(other:delta())
  local a = w:dot(w)
  local radius2 = (2 * PROXIMITY_RADIUS) * (2 * PROXIMITY_RADIUS)
  local t
  if a ~= 0.0 then
    t = -v:dot(w) / a
    if t < 0.0 then t = 0.0 elseif t > 1.0 then t = 1.0 end
  else
    t = 0.0
  end
  local d = v:plus(w:times(t))
  if d:squaredMagnitude() <= radius2 then
    return self.posOne:plus(self:delta():times(t))
  end
  return nil
end

-- Collision detector ----------------------------------------------------------------------------------------
local function voxelHash(p)
  local xDiv = floor(p.x / VOXEL_SIZE)
  local yDiv = floor(p.y / VOXEL_SIZE)
  return newV2(VOXEL_SIZE * xDiv, VOXEL_SIZE * yDiv)
end

local function isInVoxel(voxel, motion)
  local init, fin = motion.posOne, motion.posTwo
  local xLo, xHi, yLo, yHi = voxel.x - PROXIMITY_RADIUS, voxel.x + VOXEL_SIZE + PROXIMITY_RADIUS,
    voxel.y - PROXIMITY_RADIUS, voxel.y + VOXEL_SIZE + PROXIMITY_RADIUS
  local t0, t1 = 0.0, 1.0
  local dx, dy = fin.x - init.x, fin.y - init.y
  if dx == 0.0 then
    if init.x < xLo or init.x > xHi then return false end
  else
    local a, b = (xLo - init.x) / dx, (xHi - init.x) / dx
    if a > b then a, b = b, a end
    if a > t0 then t0 = a end
    if b < t1 then t1 = b end
  end
  if dy == 0.0 then
    if init.y < yLo or init.y > yHi then return false end
  else
    local a, b = (yLo - init.y) / dy, (yHi - init.y) / dy
    if a > b then a, b = b, a end
    if a > t0 then t0 = a end
    if b < t1 then t1 = b end
  end
  return t0 <= t1
end

local DIRS = { newV2(-VOXEL_SIZE, -VOXEL_SIZE), newV2(-VOXEL_SIZE, 0), newV2(-VOXEL_SIZE, VOXEL_SIZE),
  newV2(0, -VOXEL_SIZE), newV2(0, VOXEL_SIZE), newV2(VOXEL_SIZE, -VOXEL_SIZE), newV2(VOXEL_SIZE, 0),
  newV2(VOXEL_SIZE, VOXEL_SIZE) }

local function recurse(voxelMap, seen, nextVoxel, motion)
  if not isInVoxel(nextVoxel, motion) then return end
  if seen:put(nextVoxel, true) ~= nil then return end
  local list = voxelMap:get(nextVoxel)
  if list == nil then
    list = {}
    voxelMap:put(nextVoxel, list)
  end
  insert(list, motion)
  for i = 1, 8 do recurse(voxelMap, seen, nextVoxel:plus(DIRS[i]), motion) end
end

local function reduceCollisionSet(motions)
  local voxelMap = newRBT()
  for i = 1, #motions do
    local m = motions[i]
    recurse(voxelMap, newRBT(), voxelHash(m.posOne), m)
  end
  local result = {}
  voxelMap:forEach(function(k, list) if #list > 1 then insert(result, list) end end)
  return result
end

local state = newRBT()
local function handleNewFrame(frame)
  local motions = {}
  local seen = newRBT()
  for i = 1, #frame do
    local f = frame[i]
    local oldPos = state:put(f.callsign, f.pos)
    if oldPos == nil then oldPos = f.pos end
    insert(motions, newMotion(f.callsign, oldPos, f.pos))
    seen:put(f.callsign, true)
  end
  local toRemove = {}
  state:forEach(function(k, v) if seen:get(k) == nil then insert(toRemove, k) end end)
  for i = 1, #toRemove do state:remove(toRemove[i]) end
  local reduced = reduceCollisionSet(motions)
  local collisions = 0
  for r = 1, #reduced do
    local list = reduced[r]
    for i = 1, #list do
      for j = i + 1, #list do
        if list[i]:findIntersection(list[j]) ~= nil then collisions = collisions + 1 end
      end
    end
  end
  return collisions, #reduced
end

local function simulate(time, f)
  local frame = {}
  local n = (f % 10 == 5) and 90 or NUM_AIRCRAFT  -- some aircraft leave for one frame (exercises tree removal)
  for i = 1, n, 2 do
    frame[#frame + 1] = { callsign = newCallSign(i), pos = newV3(time, cos(time) * 2 + i * 3, 10) }
    frame[#frame + 1] = { callsign = newCallSign(i + 1), pos = newV3(time, sin(time) * 2 + i * 3, 10) }
  end
  return frame
end

local collisions, groups = 0, 0
for it = 1, N do
  state = newRBT()
  for f = 0, NUM_FRAMES - 1 do
    local c, g = handleNewFrame(simulate(f * 0.1, f))
    collisions = collisions + c
    groups = groups + g
  end
end
print("cd", collisions, groups)
