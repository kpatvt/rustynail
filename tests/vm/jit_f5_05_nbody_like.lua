-- F5: nbody-like float field updates (loads then stores of the same fields), with a metatable on some bodies.
local function mkbody(x, y, vx, vy, m, withmt)
  local b = {x = x, y = y, vx = vx, vy = vy, mass = m}
  if withmt then setmetatable(b, {__index = {kind = "body"}}) end
  return b
end
local bodies = {
  mkbody(0, 0, 0, 0, 39.47, false),
  mkbody(4.84, -1.16, 0.606, 2.81, 0.037, true),
  mkbody(8.34, 4.12, -1.01, 1.82, 0.011, false),
  mkbody(12.89, -15.11, 1.08, 0.868, 0.0017, true),
}
local function advance(bs, dt)
  local n = #bs
  for i = 1, n do
    local bi = bs[i]
    for j = i + 1, n do
      local bj = bs[j]
      local dx, dy = bi.x - bj.x, bi.y - bj.y
      local d2 = dx * dx + dy * dy
      local mag = dt / (d2 * math.sqrt(d2))
      bi.vx = bi.vx - dx * bj.mass * mag
      bi.vy = bi.vy - dy * bj.mass * mag
      bj.vx = bj.vx + dx * bi.mass * mag
      bj.vy = bj.vy + dy * bi.mass * mag
    end
  end
  for i = 1, n do
    local b = bs[i]
    b.x = b.x + dt * b.vx
    b.y = b.y + dt * b.vy
  end
end
for step = 1, 2000 do
  if step == 1000 then
    -- a body gains __newindex mid-run; its fields are all present, so no store reaches it
    setmetatable(bodies[3], {__newindex = function() error("no insertion expected") end})
  end
  advance(bodies, 0.01)
end
for i, b in ipairs(bodies) do print(i, string.format("%.9f %.9f %.9f %.9f", b.x, b.y, b.vx, b.vy)) end
