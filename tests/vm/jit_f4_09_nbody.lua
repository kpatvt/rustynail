-- F4: nbody-like float code with locals in the inner loop
local sqrt = math.sqrt
local bodies = {
  { x = 0, y = 0, z = 0, vx = 0, vy = 0, vz = 0, mass = 39.47 },
  { x = 4.84, y = -1.16, z = -0.10, vx = 0.606, vy = 2.81, vz = -0.02, mass = 0.037 },
  { x = 8.34, y = 4.12, z = -0.40, vx = -1.01, vy = 1.82, vz = 0.008, mass = 0.011 },
  { x = 12.89, y = -15.11, z = -0.22, vx = 1.08, vy = 0.868, vz = -0.01, mass = 0.0017 },
  { x = 15.37, y = -25.9, z = 0.179, vx = 0.979, vy = 0.594, vz = -0.034, mass = 0.002 },
}
local function advance(bodies, nbody, dt)
  for i = 1, nbody do
    local bi = bodies[i]
    local bix, biy, biz, bimass = bi.x, bi.y, bi.z, bi.mass
    local bivx, bivy, bivz = bi.vx, bi.vy, bi.vz
    for j = i + 1, nbody do
      local bj = bodies[j]
      local dx, dy, dz = bix - bj.x, biy - bj.y, biz - bj.z
      local dist2 = dx * dx + dy * dy + dz * dz
      local mag = sqrt(dist2); mag = dt / (mag * dist2)
      local bm = bj.mass * mag
      bivx = bivx - (dx * bm); bivy = bivy - (dy * bm); bivz = bivz - (dz * bm)
      bm = bimass * mag
      bj.vx = bj.vx + (dx * bm); bj.vy = bj.vy + (dy * bm); bj.vz = bj.vz + (dz * bm)
    end
    bi.vx = bivx; bi.vy = bivy; bi.vz = bivz
    bi.x = bix + dt * bivx; bi.y = biy + dt * bivy; bi.z = biz + dt * bivz
  end
end
for i = 1, 400 do advance(bodies, 5, 0.01) end
for i = 1, 5 do local b = bodies[i]; print(string.format("%.9g %.9g %.9g %.9g", b.x, b.y, b.vx, b.vz)) end
-- a body that becomes an integer / string-coerced value in the middle of the run
bodies[3].mass = 1
bodies[2].x = "4.5"
for i = 1, 50 do advance(bodies, 5, 0.01) end
print(string.format("%.9g %.9g", bodies[3].x, bodies[4].vy))
