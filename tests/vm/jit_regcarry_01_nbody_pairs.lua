-- the nbody advance loop: inner pair loop is the root, the outer iteration a side trace
local sqrt = math.sqrt
local bodies = {}
for i = 1, 5 do
  bodies[i] = { x = i * 1.5, y = i * 0.5 - 1, z = -i * 0.25, vx = 0.01 * i, vy = -0.02 * i, vz = 0.005 * i, mass = 1 + i * 0.1 }
end
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
for step = 1, 300 do advance(bodies, 5, 0.01) end
local out = {}
for i = 1, 5 do local b = bodies[i]; out[#out + 1] = string.format("%.9f %.9f %.9f", b.x, b.vy, b.z) end
print(table.concat(out, " "))
-- the same with 3 bodies (the inner loop runs once or not at all) and 7 bodies
for _, n in ipairs { 3, 4 } do
  for step = 1, 100 do advance(bodies, n, 0.01) end
  print(n, string.format("%.9f", bodies[1].x))
end
