//! R1 programs (`regcarry`: register-carrying transitions between traces, docs/JIT_TRACE.md section 16). Each has a nested loop whose inner loop is the
//! root trace and whose outer iteration is a side trace that links back into it, so that values travel in registers across exits and links; they
//! observe the stack after an exit (errors with `debug.getlocal`, hooks, GC, yields) and change types between outer iterations.

use super::progs::Prog;

pub const R1_PROGS: &[Prog] = &[
    Prog {
        name: "r1_nbody_pairs",
        small: false,
        src: r#"
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
"#,
    },
    Prog {
        name: "r1_type_flip",
        small: false,
        src: r#"
-- a slot the outer iteration carries changes type between outer iterations: the link's types no longer prove the root's entry
local function run(n, m, flip)
  local acc, k = 0, 1
  local out = {}
  for i = 1, n do
    local base = i
    local f = flip(i)
    for j = 1, m do
      acc = acc + base * j
      k = k + 1
    end
    acc = acc + f
    out[#out + 1] = math.type(acc) or type(acc)
  end
  return acc, k, table.concat(out, ",")
end
local function flip_none(i) return 1 end
local function flip_f(i) return (i % 3 == 0) and 0.5 or 1 end
local function flip_s(i) return (i % 5 == 0) and "2" or 3 end
local r = {}
for k = 1, 60 do run(8, 5, flip_none) end
for k = 1, 3 do r[#r + 1] = table.concat({ run(8, 5, flip_none) }, ";") end
for k = 1, 60 do run(8, 5, flip_f) end
for k = 1, 3 do r[#r + 1] = table.concat({ run(8, 5, flip_f) }, ";") end
for k = 1, 30 do run(8, 5, flip_s) end
for k = 1, 3 do r[#r + 1] = table.concat({ run(8, 5, flip_s) }, ";") end
for k = 1, 30 do run(8, 5, flip_none) end
r[#r + 1] = table.concat({ run(8, 5, flip_none) }, ";")
print(table.concat(r, "|"))
"#,
    },
    Prog {
        name: "r1_error_locals",
        small: false,
        src: r#"
-- an error thrown behind many carried transitions: the handler reads the locals of the dead frame (the stack must hold the carried values)
local function work(t, n, bad)
  local sum, cnt, scale = 0.0, 0, 1.5
  for i = 1, n do
    local row = t[i]
    local a, b = row.a, row.b
    for j = 1, 3 do
      sum = sum + a * j + b
      cnt = cnt + 1
    end
    scale = scale + 0.25
    if i == bad then local x = row.missing + 1 end
  end
  return sum, cnt, scale
end
local t = {}
for i = 1, 40 do t[i] = { a = i * 0.5, b = i % 7 } end
for k = 1, 80 do work(t, 40, -1) end
local out = {}
for _, bad in ipairs { 3, 17, 40, 1 } do
  local ok, e = xpcall(work, function(m)
    local r = {}
    for l = 1, 12 do
      local name, v = debug.getlocal(2, l)
      if name then r[#r + 1] = name .. "=" .. (type(v) == "table" and "tbl" or tostring(v)) end
    end
    return table.concat(r, ",") .. " | " .. tostring(m)
  end, t, 40, bad)
  out[#out + 1] = tostring(ok) .. " " .. tostring(e)
end
print(table.concat(out, "\n"))
"#,
    },
    Prog {
        name: "r1_hook_locals",
        small: false,
        src: r#"
-- count hooks fire in the nested loop and read its locals (the interrupt exits behind carried transitions)
local seen = {}
local function work(n)
  local s, c, t = 0, 0, 0.5
  for i = 1, n do
    local u = i * 2
    local f = i + 0.25
    for j = 1, 4 do
      s = s + u + j
      c = c + 1
      t = t + f
    end
  end
  return s, c, t
end
for k = 1, 60 do work(20) end
local cnt = 0
debug.sethook(function()
  cnt = cnt + 1
  if cnt % 7 == 0 then
    local r = {}
    for l = 1, 10 do
      local name, v = debug.getlocal(2, l)
      if name then r[#r + 1] = name .. "=" .. (type(v) == "table" and "tbl" or tostring(v)) end
    end
    seen[#seen + 1] = table.concat(r, ",")
  end
end, "", 53)
local a, b, c = work(25)
debug.sethook()
print(a, b, c, cnt, #seen)
print(table.concat(seen, "\n", 1, math.min(#seen, 12)))
"#,
    },
    Prog {
        name: "r1_gc_coroutine",
        small: false,
        src: r#"
-- the outer iteration allocates (GC steps at exits), a coroutine yields behind carried transitions, locals hold tables and strings
local function gen(n)
  local keep = {}
  local total, s = 0, ""
  for i = 1, n do
    local obj = { id = i, name = "n" .. i }
    local w = i * 0.5
    for j = 1, 3 do
      total = total + obj.id * j
      w = w + j
    end
    keep[#keep + 1] = obj
    if i % 4 == 0 then collectgarbage("step", 1) end
    s = obj.name
    coroutine.yield(total, w, s)
    if #keep > 5 then keep = {} end
  end
  return "done", total
end
local res = {}
for rep = 1, 40 do
  local co = coroutine.wrap(gen)
  local last
  for i = 1, 14 do last = table.concat({ tostring((co(i == 1 and 13 or nil))) }, "") end
  if rep % 10 == 0 then res[#res + 1] = last end
end
local co = coroutine.wrap(gen)
local o = {}
for i = 1, 8 do
  local a, b, c = co(i == 1 and 7 or nil)
  o[#o + 1] = tostring(a) .. "/" .. tostring(b) .. "/" .. tostring(c)
end
print(table.concat(res, " "), table.concat(o, " "))
"#,
    },
    Prog {
        name: "r1_many_slots",
        small: false,
        src: r#"
-- more live numeric locals than registers: carried values live in spill slots, with a table, an integer and a float among them
local function f(t, n)
  local a1, a2, a3, a4, a5, a6, a7, a8 = 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0
  local b1, b2, b3, b4, b5, b6, b7, b8 = 0.5, 0.25, 0.125, 1.5, 2.5, 3.5, 4.5, 5.5
  local c1, c2, c3 = 10, 20, 30
  for i = 1, n do
    local row = t[i]
    local x = row.x
    for j = 1, 2 do
      a1 = a1 + x * b1; a2 = a2 + x * b2; a3 = a3 + x * b3; a4 = a4 + x * b4
      a5 = a5 + x * b5; a6 = a6 + x * b6; a7 = a7 + x * b7; a8 = a8 + x * b8
      b1 = b1 + a8 * 1e-3; b2 = b2 + a7 * 1e-3; b3 = b3 + a6 * 1e-3; b4 = b4 + a5 * 1e-3
      c1 = c1 + j; c2 = c2 + i; c3 = c3 + c1 % 3
    end
    b5 = b5 + a1 * 1e-4; b6 = b6 + a2 * 1e-4; b7 = b7 + a3 * 1e-4; b8 = b8 + a4 * 1e-4
  end
  return a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8, b1 + b2 + b3 + b4 + b5 + b6 + b7 + b8, c1, c2, c3
end
local t = {}
for i = 1, 30 do t[i] = { x = i * 0.1 } end
for k = 1, 60 do f(t, 30) end
for _, n in ipairs { 30, 7, 1, 0, 30 } do print(n, string.format("%.6f %.6f", f(t, n)), select(3, f(t, n))) end
"#,
    },
    Prog {
        name: "r1_copy_slots",
        small: false,
        src: r#"
-- slots copied by MOVE (two slots name one value), carried across the outer iteration, a table kept in a local, an unread local that is only overwritten
local function f(t, n)
  local a, unused = 0.5, 7.5
  local keep = t
  local x1, x2 = 1.0, 2.0
  for i = 1, n do
    local c1, c2 = x1, x2
    unused = c2
    x1 = x1 + 0.25
    for j = 1, 3 do
      a = a + c1 * j + c2
    end
    x2 = c1 + c2 * 0.5
    keep = keep
  end
  return a, x1, x2, unused, keep == t
end
local t = {}
for k = 1, 60 do f(t, 12) end
for _, n in ipairs { 12, 5, 1, 0 } do print(n, f(t, n)) end
"#,
    },
];
