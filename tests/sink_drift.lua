-- S0 check (4): collectgarbage("count") with allocation sinking. Prints one line per sample: kind, index, count (KB).
-- kind F = right after a full collection (must be equal across modes up to a few hundred bytes), kind R = at a pseudo
-- random point (bounded drift). Driven by scripts/sink-drift.py, which compares the modes.
local mode = arg and arg[1] or "incremental"
collectgarbage(mode)
local keep = {}
local function work(n, seedv)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = i + seedv, z = 0.5 }
    local w = { x = v.y, y = v.x + 2 }
    if i % 211 == 0 then keep[#keep % 64 + 1] = v end
    s = s + v.x * w.y - w.x
  end
  return s
end
local lcg = 12345
local function rnd(m) lcg = (lcg * 1103515245 + 12345) % 2147483648 return lcg % m end
local out = {}
local total = 0
for k = 1, 1000 do
  total = total + work(50 + rnd(400), k)
  out[#out + 1] = string.format("R %d %.3f", k, collectgarbage("count"))
  if k % 100 == 0 then
    collectgarbage()
    out[#out + 1] = string.format("F %d %.3f", k, collectgarbage("count"))
  end
end
print(table.concat(out, "\n"))
print("total", total)
