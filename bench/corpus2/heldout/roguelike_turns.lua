-- roguelike_turns: imitates a small roguelike game loop (rooms+corridors map, closure-object
-- monsters with string-state AI, ray-cast FOV, combat log, time-based turn scheduler). small: N=1
local N = tonumber(arg and arg[1]) or 115

local W, H = 64, 28
local WALL, FLOOR = 1, 0
local seed = 20240607
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

local function new_map()
  local m = {}
  for y = 1, H do
    local row = {}
    for x = 1, W do row[x] = WALL end
    m[y] = row
  end
  return m
end

local function carve_room(m, r)
  for y = r.y, r.y + r.h - 1 do
    for x = r.x, r.x + r.w - 1 do m[y][x] = FLOOR end
  end
end

local function carve_h(m, x1, x2, y)
  for x = math.min(x1, x2), math.max(x1, x2) do m[y][x] = FLOOR end
end

local function carve_v(m, y1, y2, x)
  for y = math.min(y1, y2), math.max(y1, y2) do m[y][x] = FLOOR end
end

local function gen_level()
  local m, rooms = new_map(), {}
  for _ = 1, 40 do
    local w, h = rnd(8) + 3, rnd(5) + 3
    local x, y = rnd(W - w - 2) + 1, rnd(H - h - 2) + 1
    local ok = true
    for _, r in ipairs(rooms) do
      if x < r.x + r.w + 1 and r.x < x + w + 1 and y < r.y + r.h + 1 and r.y < y + h + 1 then
        ok = false
        break
      end
    end
    if ok then
      local r = { x = x, y = y, w = w, h = h, cx = x + math.floor(w / 2), cy = y + math.floor(h / 2) }
      carve_room(m, r)
      table.insert(rooms, r)
    end
  end
  for i = 2, #rooms do
    local a, b = rooms[i - 1], rooms[i]
    if rnd(2) == 1 then
      carve_h(m, a.cx, b.cx, a.cy); carve_v(m, a.cy, b.cy, b.cx)
    else
      carve_v(m, a.cy, b.cy, a.cx); carve_h(m, a.cx, b.cx, b.cy)
    end
  end
  return m, rooms
end
local function line_clear(m, x0, y0, x1, y1)
  local dx, dy = math.abs(x1 - x0), math.abs(y1 - y0)
  local sx = x0 < x1 and 1 or -1
  local sy = y0 < y1 and 1 or -1
  local err = dx - dy
  local x, y = x0, y0
  while not (x == x1 and y == y1) do
    if (x ~= x0 or y ~= y0) and m[y][x] == WALL then return false end
    local e2 = 2 * err
    if e2 > -dy then err = err - dy; x = x + sx end
    if e2 < dx then err = err + dx; y = y + sy end
  end
  return true
end

local function compute_fov(m, px, py, radius)
  local seen, count = {}, 0
  for y = math.max(1, py - radius), math.min(H, py + radius) do
    for x = math.max(1, px - radius), math.min(W, px + radius) do
      local dx, dy = x - px, y - py
      if dx * dx + dy * dy <= radius * radius and line_clear(m, px, py, x, y) then
        seen[y * 100 + x] = true
        count = count + 1
      end
    end
  end
  return seen, count
end

local log_ring, log_counts, log_total = {}, {}, 0
local function log(kind, text)
  log_total = log_total + 1
  log_counts[kind] = (log_counts[kind] or 0) + 1
  table.insert(log_ring, text)
  if #log_ring > 6 then table.remove(log_ring, 1) end
end

local world
local DIRS = { {-1,-1}, {0,-1}, {1,-1}, {-1,0}, {1,0}, {-1,1}, {0,1}, {1,1} }
local function passable(x, y)
  return x >= 1 and x <= W and y >= 1 and y <= H and world.map[y][x] == FLOOR and not world.occ[y * 100 + x]
end
local function step_towards(x, y, tx, ty, away)
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

local function wander_step(x, y)
  local start = rnd(8)
  for i = 0, 7 do
    local d = DIRS[(start + i - 1) % 8 + 1]
    if passable(x + d[1], y + d[2]) then return x + d[1], y + d[2] end
  end
  return x, y
end
local KINDS = {
  rat    = { hp = 6,  atk = 2, delay = 80,  sight = 6, flee_at = 2 },
  goblin = { hp = 12, atk = 4, delay = 100, sight = 8, flee_at = 3 },
  orc    = { hp = 22, atk = 6, delay = 130, sight = 7, flee_at = 4 },
  bat    = { hp = 4,  atk = 1, delay = 60,  sight = 9, flee_at = 0 },
}
local KIND_LIST = { "rat", "goblin", "orc", "bat" }
local function make_monster(kind, id, x, y)
  local def = KINDS[kind]
  local hp, state = def.hp, "wander"
  local self = { id = id, kind = kind, next_time = id }
  world.occ[y * 100 + x] = self
  local function move_to(nx, ny)
    if nx ~= x or ny ~= y then
      world.occ[y * 100 + x] = nil
      x, y = nx, ny
      world.occ[y * 100 + x] = self
    end
  end
  local handlers = {
    wander = function(p)
      move_to(wander_step(x, y))
    end,
    chase = function(p)
      if math.abs(p.x - x) <= 1 and math.abs(p.y - y) <= 1 then
        local dmg = rnd(def.atk)
        if rnd(100) <= 70 then
          p.hp = p.hp - dmg
          log("hit", kind .. self.id .. " hits you for " .. dmg)
        else
          log("miss", kind .. self.id .. " misses")
        end
      else
        move_to(step_towards(x, y, p.x, p.y, false))
      end
    end,
    flee = function(p)
      move_to(step_towards(x, y, p.x, p.y, true))
      if hp < def.hp then hp = hp + (rnd(4) == 1 and 1 or 0) end
    end,
  }

  function self.pos() return x, y end
  function self.alive() return hp > 0 end
  function self.hp() return hp end
  function self.state() return state end
  function self.hurt(n)
    hp = hp - n
    if hp > 0 and hp <= def.flee_at then state = "flee" end
    return hp
  end
  function self.die() world.occ[y * 100 + x] = nil end
  function self.act(p)
    local sees = false
    local dx, dy = p.x - x, p.y - y
    if dx * dx + dy * dy <= def.sight * def.sight then
      sees = line_clear(world.map, x, y, p.x, p.y)
    end
    if state ~= "flee" then state = sees and "chase" or "wander" end
    if state == "flee" and hp >= def.hp - 1 then state = "wander" end
    handlers[state](p)
    return def.delay
  end
  return self
end

local function player_act(p)
  local seen, cnt = compute_fov(world.map, p.x, p.y, 8)
  p.fov_total = p.fov_total + cnt
  local target, tdist
  for _, m in ipairs(world.monsters) do
    local mx, my = m.pos()
    if seen[my * 100 + mx] then
      local d = (mx - p.x) ^ 2 + (my - p.y) ^ 2
      if not target or d < tdist or (d == tdist and m.id < target.id) then target, tdist = m, d end
    end
  end
  if target then
    local mx, my = target.pos()
    if tdist <= 2 then
      local dmg = rnd(6) + 1
      if rnd(100) <= 80 then
        local left = target.hurt(dmg)
        log("attack", "you hit " .. target.kind .. target.id .. " for " .. dmg)
        if left <= 0 then
          log("kill", "you kill " .. target.kind .. target.id)
          p.kills = p.kills + 1
        end
      else
        log("swing", "you miss " .. target.kind .. target.id)
      end
    else
      p.x, p.y = step_towards(p.x, p.y, mx, my, false)
    end
  else
    if p.x == p.gx and p.y == p.gy or rnd(10) == 1 then
      local r = world.rooms[rnd(#world.rooms)]
      p.gx, p.gy = r.cx, r.cy
    end
    local nx, ny = step_towards(p.x, p.y, p.gx, p.gy, false)
    if nx == p.x and ny == p.y then nx, ny = wander_step(p.x, p.y) end
    p.x, p.y = nx, ny
    if p.hp < p.maxhp and rnd(3) == 1 then p.hp = p.hp + 1 end
  end
  return 100
end
local function run_level(depth, player)
  local m, rooms = gen_level()
  world = { map = m, rooms = rooms, occ = {}, monsters = {} }
  player.x, player.y = rooms[1].cx, rooms[1].cy
  player.gx, player.gy = player.x, player.y
  player.hp = player.maxhp
  local nmon = 6 + depth
  for i = 1, nmon do
    local r = rooms[rnd(#rooms - 1) + 1]
    local x, y = r.x + rnd(r.w) - 1, r.y + rnd(r.h) - 1
    if passable(x, y) then
      table.insert(world.monsters, make_monster(KIND_LIST[rnd(4)], i, x, y))
    end
  end
  local start_count = #world.monsters
  local pnext, now, turns = 0, 0, 0
  while now < 6000 and player.hp > 0 and #world.monsters > 0 do
    local who, wi = nil, nil
    for i, mo in ipairs(world.monsters) do
      if not who or mo.next_time < who.next_time or (mo.next_time == who.next_time and mo.id < who.id) then
        who, wi = mo, i
      end
    end
    if pnext <= who.next_time then
      now = pnext
      pnext = pnext + player_act(player)
      turns = turns + 1
      for i = #world.monsters, 1, -1 do
        local mo = world.monsters[i]
        if not mo.alive() then mo.die(); table.remove(world.monsters, i) end
      end
    else
      now = who.next_time
      who.next_time = who.next_time + who.act(player)
      turns = turns + 1
    end
  end
  local states = { wander = 0, chase = 0, flee = 0 }
  for _, mo in ipairs(world.monsters) do states[mo.state()] = states[mo.state()] + 1 end
  return start_count, #world.monsters, turns, states
end
local player = { x = 1, y = 1, hp = 30, maxhp = 30, kills = 0, fov_total = 0, gx = 1, gy = 1 }
local total_turns, deaths = 0, 0
for depth = 1, N do
  local s, left, turns, st = run_level(depth, player)
  total_turns = total_turns + turns
  if player.hp <= 0 then deaths = deaths + 1 end
  local floor_cells = 0
  for y = 1, H do for x = 1, W do if world.map[y][x] == FLOOR then floor_cells = floor_cells + 1 end end end
  if depth <= 5 or depth == N then
    print(string.format("level %2d: rooms=%d floor=%d monsters %d->%d turns=%d hp=%d w/c/f=%d/%d/%d",
      depth, #world.rooms, floor_cells, s, left, turns, player.hp, st.wander, st.chase, st.flee))
  end
end
print(string.format("total turns=%d deaths=%d kills=%d fov_calls=%d", total_turns, deaths, player.kills, player.fov_total))
local kinds = {}
for k in pairs(log_counts) do kinds[#kinds + 1] = k end
table.sort(kinds)
for _, k in ipairs(kinds) do print(string.format("log %-7s %d", k, log_counts[k])) end
print("log total", log_total)
for i, t in ipairs(log_ring) do print(i, t) end
for y = 1, H, 3 do
  local row = {}
  for x = 1, W, 2 do row[#row + 1] = world.map[y][x] == WALL and "#" or "." end
  print(table.concat(row))
end
