-- Table bucket: GETFIELD/SETFIELD of numeric fields on a few tables (nbody-like; no sqrt call, so the hot
-- function has no native calls).
local function f(bodies, n, steps, dt)
  for s = 1, steps do
    for i = 1, n do
      local bi = bodies[i]
      local bix, biy, biz = bi.x, bi.y, bi.z
      local bimass = bi.mass
      local vx, vy, vz = bi.vx, bi.vy, bi.vz
      for j = i + 1, n do
        local bj = bodies[j]
        local dx, dy, dz = bix - bj.x, biy - bj.y, biz - bj.z
        local d2 = dx * dx + dy * dy + dz * dz
        local mag = dt / (d2 * (d2 + 1.0))
        local bm = bj.mass * mag
        vx = vx - dx * bm
        vy = vy - dy * bm
        vz = vz - dz * bm
        local im = bimass * mag
        bj.vx = bj.vx + dx * im
        bj.vy = bj.vy + dy * im
        bj.vz = bj.vz + dz * im
      end
      bi.vx, bi.vy, bi.vz = vx, vy, vz
    end
    for i = 1, n do
      local b = bodies[i]
      b.x = b.x + dt * b.vx
      b.y = b.y + dt * b.vy
      b.z = b.z + dt * b.vz
    end
  end
  local e = 0.0
  for i = 1, n do
    local b = bodies[i]
    e = e + b.x + b.y + b.z + b.vx * b.vx + b.vy * b.vy + b.vz * b.vz
  end
  return e
end

local function setup(steps)
  local n = 5
  local bodies = {}
  for i = 1, n do
    bodies[i] = {
      x = i * 1.5, y = (i * 7 % 5) * 1.25, z = (i * 3 % 4) * 0.75,
      vx = 0.0, vy = 0.0, vz = 0.0,
      mass = 1.0 + i * 0.25,
    }
  end
  return bodies, n, steps, 0.01
end

if ORACLE then return f, setup end
print(f(setup(tonumber(arg and arg[1]) or 300000)))
