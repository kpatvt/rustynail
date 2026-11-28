-- docs/JIT_TRACE.md section 81.2 (`eflazy`): a loop whose live-ins change type between entries (`best` is nil when the loop starts and a
-- number after its first improvement, `away` is true or false per call). A root recorded in the middle of the loop checks `best` as a number
-- and `away` as one boolean at its entry, so most entries fail and the head is re-recorded until it is blacklisted; with the knob the failing
-- slots are checked at their first use. Outputs against the interpreter.
local DIRS = { { -1, -1 }, { 0, -1 }, { 1, -1 }, { -1, 0 }, { 1, 0 }, { -1, 1 }, { 0, 1 }, { 1, 1 } }
local W, H = 40, 20
local blocked = {}
for i = 1, W * H do blocked[i] = (i * 7) % 11 == 0 end
local function passable(x, y)
  return x >= 1 and x <= W and y >= 1 and y <= H and not blocked[(y - 1) * W + x]
end
local function step(x, y, tx, ty, away)
  local best, bx, by = nil, x, y
  for _, d in ipairs(DIRS) do
    local nx, ny = x + d[1], y + d[2]
    if passable(nx, ny) then
      local dist = (nx - tx) ^ 2 + (ny - ty) ^ 2
      if away then dist = -dist end
      if not best or dist < best then best, bx, by = dist, nx, ny end
    end
  end
  return bx, by
end
local sx, sy, acc = 0, 0, 0
for i = 1, 40000 do
  local x, y = i % W + 1, i % H + 1
  local nx, ny = step(x, y, (i * 3) % W + 1, (i * 5) % H + 1, i % 3 == 0)
  sx, sy = sx + nx, sy + ny
  if i % 997 == 0 then acc = acc + sx % 13 end
end
print(sx, sy, acc)
