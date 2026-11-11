-- NBody: five-body solar system (port from the public are-we-fast-yet "NBody" description: Body objects with
-- fields, NBodySystem with advance/energy). Output: energy before and after, %.9f.
-- Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
local N = tonumber(arg and arg[1]) or 250000
local sqrt = math.sqrt
local PI = 3.141592653589793
local SOLAR_MASS = 4 * PI * PI
local DAYS_PER_YEAR = 365.24

local Body = {}
Body.__index = Body
local function newBody(x, y, z, vx, vy, vz, mass)
  return setmetatable({ x = x, y = y, z = z, vx = vx * DAYS_PER_YEAR, vy = vy * DAYS_PER_YEAR,
    vz = vz * DAYS_PER_YEAR, mass = mass * SOLAR_MASS }, Body)
end
function Body:offsetMomentum(px, py, pz)
  self.vx = 0 - px / SOLAR_MASS
  self.vy = 0 - py / SOLAR_MASS
  self.vz = 0 - pz / SOLAR_MASS
end

local function newSystem()
  local bodies = {
    setmetatable({ x = 0, y = 0, z = 0, vx = 0, vy = 0, vz = 0, mass = SOLAR_MASS }, Body),
    newBody(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
      1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04),
    newBody(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
      -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04),
    newBody(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
      2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05),
    newBody(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
      2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05),
  }
  local px, py, pz = 0.0, 0.0, 0.0
  for i = 1, #bodies do
    local b = bodies[i]
    px = px + b.vx * b.mass
    py = py + b.vy * b.mass
    pz = pz + b.vz * b.mass
  end
  bodies[1]:offsetMomentum(px, py, pz)
  return bodies
end

local function advance(bodies, dt)
  local n = #bodies
  for i = 1, n do
    local ib = bodies[i]
    for j = i + 1, n do
      local jb = bodies[j]
      local dx, dy, dz = ib.x - jb.x, ib.y - jb.y, ib.z - jb.z
      local dSq = dx * dx + dy * dy + dz * dz
      local distance = sqrt(dSq)
      local mag = dt / (dSq * distance)
      ib.vx = ib.vx - dx * jb.mass * mag
      ib.vy = ib.vy - dy * jb.mass * mag
      ib.vz = ib.vz - dz * jb.mass * mag
      jb.vx = jb.vx + dx * ib.mass * mag
      jb.vy = jb.vy + dy * ib.mass * mag
      jb.vz = jb.vz + dz * ib.mass * mag
    end
  end
  for i = 1, n do
    local b = bodies[i]
    b.x = b.x + dt * b.vx
    b.y = b.y + dt * b.vy
    b.z = b.z + dt * b.vz
  end
end

local function energy(bodies)
  local e = 0.0
  local n = #bodies
  for i = 1, n do
    local ib = bodies[i]
    e = e + 0.5 * ib.mass * (ib.vx * ib.vx + ib.vy * ib.vy + ib.vz * ib.vz)
    for j = i + 1, n do
      local jb = bodies[j]
      local dx, dy, dz = ib.x - jb.x, ib.y - jb.y, ib.z - jb.z
      e = e - (ib.mass * jb.mass) / sqrt(dx * dx + dy * dy + dz * dz)
    end
  end
  return e
end

local bodies = newSystem()
local e0 = energy(bodies)
for i = 1, N do advance(bodies, 0.01) end
print(string.format("nbody %.9f %.9f", e0, energy(bodies)))
