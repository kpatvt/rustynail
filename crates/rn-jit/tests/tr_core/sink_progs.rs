//! S0: programs of allocation sinking (docs/JIT_TRACE.md 6.8), kept apart from `progs.rs`.

use super::progs::Prog;

pub const SINK_PROGS: &[Prog] = &[
    Prog {
        name: "sink_basic",
        small: true,
        src: r#"
-- temporary vectors that never leave the loop body (allocation sinking, S0)
local function f(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = i + 1 }
    local w = { x = v.y, y = v.x + 2.5 }
    v.x = v.x * 2
    s = s + v.x * w.y - w.x + v.y
  end
  return s
end
local a = 0
for k = 1, 30 do a = a + f(40 + k) end
print(a, f(3))
"#,
    },
    Prog {
        name: "sink_exit_mid",
        small: true,
        src: r#"
-- the guard that sometimes fails sits after the table is built: the exit materialises it (fields, updates, strings,
-- booleans, an older table as a field value); the interpreter then stores it in `keep`
local older = { tag = "old" }
local keep = {}
local function f(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = i * 0.5, name = "n", flag = i > 3, ref = older }
    v.x = v.x + 1
    if i % 9 == 0 then keep[#keep + 1] = v end
    s = s + v.x + v.y
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(30 + k) end
local t = 0
for i = 1, #keep do local v = keep[i] t = t + v.x + v.y + (v.flag and 1 or 0) + #v.name + #v.ref.tag end
print(r, #keep, t)
local ks = {}
for k, v in pairs(keep[1]) do ks[#ks + 1] = tostring(k) end
table.sort(ks)
print(table.concat(ks, " "))
"#,
    },
    Prog {
        name: "j1_sinkf",
        small: true,
        src: r#"
-- J1-2: a sunk table is live in the caller while a guard fails inside the inlined callee (frameful exits that materialise): a
-- branch on the iteration, a field of another type, a string field, two tables with an alias, a call that raises, floats and integers
local function norm(v, k)
  if k % 7 == 0 then return v.x * 2 end
  return v.x + v.y
end
local function kind(t, k)
  if k % 5 == 0 then return t.s .. "!" end
  return t.s
end
local function boom(t, k)
  if k == 400 then error("boom " .. t.x) end
  return t.x
end
local function run(n)
  local acc, str = 0, 0
  for i = 1, n do
    local v = { x = i, y = i * 0.5 }
    local w = { s = "k" .. (i % 3), x = i + 0.25, y = i, v = v }
    local a = norm(v, i)
    local b = norm(w, i)
    str = str + #kind(w, i)
    acc = acc + a + b + w.v.x
    if i % 50 == 0 then acc = acc + v.y + w.x end
    if i == n // 2 then v.y = 7 end
  end
  return acc, str
end
local x, y = 0, 0
for r = 1, 8 do local a, b = run(300 + r); x = x + a; y = y + b end
print(x, y)
print(pcall(function() local t = { x = 3, y = 4 }; local s = 0; for i = 1, 600 do s = s + boom(t, i) end; return s end))
local co = coroutine.wrap(function(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = 0.5 }
    s = s + norm(v, i)
    if i % 100 == 0 then coroutine.yield(v) end
  end
  return s
end)
local r1 = co(450)
print(type(r1), r1.x)
local r2 = co()
print(type(r2) == "table" and r2.x or r2)
local r3 = co()
print(type(r3) == "table" and r3.x or r3)
"#,
    },
    Prog {
        name: "sinkf_side",
        small: true,
        src: r#"
-- SINK-F: a sunk metatabled temporary is live across a class-polymorphic method call inside an inlined frame: the exit inside the frame
-- gets a side trace although its snapshot names a sunk table. Sections: plain frames, a metamethod frame (`__add` / `__unm` called
-- with the temporary live), an error raised in the callee, a coroutine yield while the temporary is live, `__gc` added to the
-- metatable late (the temporaries made afterwards are finalizable), a table that escapes through the side trace.
local Base = {}; Base.__index = Base
local Heavy = setmetatable({}, Base); Heavy.__index = Heavy
local V = {}; V.__index = V
local function vec(x, y) local o = setmetatable({}, V); o.x = x; o.y = y; return o end
V.__unm = function(a) return vec(-a.x, -a.y) end
V.__add = function(a, b) return vec(a.x + b.x, a.y + b.y) end
function Base:push(f) self.fx = self.fx + f.x; self.fy = self.fy + f.y end
function Heavy:push(f) self.fx = self.fx + 2 * f.x; self.fy = self.fy + f.y end
local keep
function Base:grab(f) if self.fx % 17 == 0 then keep = f end; self.fx = self.fx + 1 end
function Heavy:grab(f) self.fy = self.fy + f.y end
function Base:bad(f) if f.x == 777 then error("bad " .. f.y) end; self.fx = self.fx + f.x end
function Heavy:bad(f) self.fy = self.fy + f.y end
local objs = {}
local function reset()
  for i = 1, 24 do objs[i] = setmetatable({ fx = 0, fy = 0 }, (i % 5 == 0) and Heavy or Base) end
end
local function total()
  local s = 0
  for i = 1, 24 do s = s + objs[i].fx + objs[i].fy end
  return s
end
local function apply(a, b, k)
  local f = vec(k, k * 2)
  a:push(f)
  b:push(-f)
end
local function run1(n)
  for k = 0, n - 1 do apply(objs[k % 24 + 1], objs[(k + 7) % 24 + 1], k % 3) end
end
local function run2(n)
  for k = 0, n - 1 do
    local f = vec(k % 5, 1)
    local g = f + vec(1, k % 3)
    objs[k % 24 + 1]:push(g)
    objs[(k + 5) % 24 + 1]:push(f + g)
  end
end
local function run3(n)
  for k = 0, n - 1 do
    local f = vec(k % 7, 2)
    objs[k % 24 + 1]:grab(f)
  end
end
local function run4(n, bad)
  for k = 0, n - 1 do
    local f = vec(k == bad and 777 or k % 9, k)
    objs[k % 24 + 1]:bad(f)
  end
end
reset()
for r = 1, 10 do run1(120) end
print("plain", total())
reset()
for r = 1, 10 do run2(120) end
print("mm", total())
reset()
for r = 1, 10 do run3(120) end
print("grab", total(), keep and keep.x, keep and keep.y)
reset()
for r = 1, 6 do run4(120, -1) end
print("bad-ok", total())
print("bad", pcall(run4, 120, 100))
print("bad-after", total())
-- a coroutine yields (the temporary of the suspended frame is live); resumed many times
reset()
local co = coroutine.wrap(function(n)
  for k = 0, n - 1 do
    local f = vec(k % 4, 3)
    objs[k % 24 + 1]:push(f)
    if k % 40 == 39 then coroutine.yield(f.x + f.y) end
  end
  return -1
end)
local ys = 0
for i = 1, 40 do local y = co(480); ys = ys + y; if y < 0 then break end end
print("co", ys, total())
-- `__gc` appears on the metatable after the loop is hot: the temporaries made from here on are finalizable
reset()
for r = 1, 8 do run1(120) end
local ngc = 0
collectgarbage("stop")
V.__gc = function(o) ngc = ngc + 1 end
for r = 1, 4 do run1(120) end
collectgarbage("restart")
collectgarbage(); collectgarbage()
print("gc", total(), ngc)
V.__gc = nil
"#,
    },
    Prog {
        name: "sinkf_side_mem_error",
        small: true,
        src: r#"
-- SINK-F: as `j1_sinkf_mem_error`, with a class-polymorphic callee so that the exits inside the inlined frame get side traces (the stub
-- materialises, then continues in the side trace with the frames virtual): with an injected host allocation failure exactly one call
-- raises "not enough memory" and every other call is exact
local Base = {}; Base.__index = Base
local Heavy = setmetatable({}, Base); Heavy.__index = Heavy
local V = {}; V.__index = V
local function vec(x, y) local o = setmetatable({}, V); o.x = x; o.y = y; return o end
function Base:push(f) self.fx = self.fx + f.x end
function Heavy:push(f) self.fx = self.fx + 2 * f.x end
local objs = {}
local function reset()
  for i = 1, 16 do objs[i] = setmetatable({ fx = 0 }, (i % 4 == 0) and Heavy or Base) end
end
local function f(n)
  reset()
  for k = 0, n - 1 do
    local v = vec(k % 5, 1)
    objs[k % 16 + 1]:push(v)
  end
  local s = 0
  for i = 1, 16 do s = s + objs[i].fx end
  return s
end
local function want(n)
  local s = 0
  for k = 0, n - 1 do
    local c = (k % 16 + 1) % 4 == 0 and 2 or 1
    s = s + c * (k % 5)
  end
  return s
end
local okall, nfail, msg = true, 0, nil
for k = 1, 40 do
  local n = 200 + k
  local ok, e = pcall(f, n)
  if ok then
    if e ~= want(n) then okall = false end
  else
    nfail = nfail + 1
    msg = e
  end
end
print(okall, nfail, msg)
"#,
    },
    Prog {
        name: "sinkroot_pre",
        small: false,
        src: r#"
-- SINK-R: a filter loop over a producer closure: the record built by the producer is sunk, the predicate's exit (inside inlined frames)
-- leaves the root's pre-roll with the record live; the ineffective-root rule re-records the root once without sinking the record
local seed = 5551212
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end
local KINDS = { "temp", "humidity", "pressure", "light" }
local function readings(n)
  local i = 0
  return function()
    if i >= n then return nil end
    i = i + 1
    local k = rnd(4)
    return { t = i, kind = KINDS[k], value = (i * 7) % 97 }
  end
end
local function filter(pred, src)
  return function()
    while true do
      local v = src()
      if v == nil then return nil end
      if pred(v) then return v end
    end
  end
end
local function count(src)
  local n, s = 0, 0
  for v in src do n = n + 1; s = s + v.value end
  return n, s
end
local out = {}
for round = 1, 2 do
  for _, kind in ipairs(KINDS) do
    local n, s = count(filter(function(r) return r.kind == kind end, readings(3000)))
    out[#out + 1] = kind .. ":" .. n .. ":" .. s
  end
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "sink_alias_frames",
        small: true,
        src: r#"
-- the same sunk table in several slots (a copy, a callee's parameter), exits inside the inlined callee
local keep
local function len2(p) local d = p.x * p.x + p.y * p.y if d > 1e9 then keep = p end return d end
local function mk(a, b) local t = { x = a, y = b } return t end
local function f(n)
  local s = 0
  for i = 1, n do
    local v = mk(i, i + 1)
    local w = v
    s = s + len2(w) + len2(v)
    if i % 17 == 0 then keep = w end
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(40 + k) end
print(r, keep and keep.x, keep and keep.y, keep == nil)
"#,
    },
    Prog {
        name: "sink_live_local",
        small: true,
        src: r#"
-- a table held by a local the next iteration (and the code after the loop) reads is not dead: it must stay exact
local function f(n)
  local acc = { x = 0, y = 1 }
  for i = 1, n do
    acc = { x = acc.x + i, y = acc.y + 1 }
  end
  return acc.x, acc.y
end
local function g(n)
  local last
  for i = 1, n do
    local t = { a = i }
    last = t
  end
  return last.a
end
local a, b = 0, 0
for k = 1, 30 do local x, y = f(40 + k) a = a + x b = b + y end
local c = 0
for k = 1, 30 do c = c + g(30 + k) end
print(a, b, c, f(0), g(1))
"#,
    },
    Prog {
        name: "sink_hooks_getlocal",
        small: true,
        src: r#"
-- debug.getlocal of the live locals at an exit in the middle of the iteration
local seen = 0
local function f(n)
  local base = 7
  local s = 0
  for i = 1, n do
    local v = { x = i, y = base }
    v.x = v.x + 1
    v.y = v.y * 2
    if i % 13 == 0 then
      local nm, val = debug.getlocal(1, 2)
      seen = seen + (val and 1 or 0)
      local nm4, vv = debug.getlocal(1, 6)
      if type(vv) == "table" then seen = seen + vv.x end
    end
    s = s + v.x + v.y
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(40 + k) end
print(r, seen)
"#,
    },
    Prog {
        name: "sink_tail_multret",
        small: true,
        src: r#"
-- F4 interplay: a fresh table carried through a tail call (its slot is moved down), a recursion that builds one per level,
-- and a sunk table alive in a snapshot that carries an explicit top (a call with B = 0 / C = 0 around it); exits materialise it
local keep
local function g(p, n)
  if n == 0 then
    if p.x % 7 == 0 then keep = p end
    return p.x + p.y
  end
  return g({ x = p.x + 1, y = p.y }, n - 1)
end
local function rec(n, acc)
  if n == 0 then return acc end
  local t = { v = n }
  return rec(n - 1, acc + t.v)
end
local function two(a)
  local t = { x = a, y = a + 1 }
  if a % 11 == 0 then keep = t end
  return t.x, t.y
end
local function f(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = 1 }
    s = s + g(v, 3) + rec(4, 0) + select(2, two(i)) + math.max(two(i))
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(30 + k) end
print(r, keep and keep.x, keep and keep.y)
"#,
    },
    Prog {
        name: "sink_count_stopped",
        small: false,
        src: r#"
-- with the collector stopped, the bytes allocated by a sinking loop (virtual or materialised) equal the interpreter's
collectgarbage("stop")
local keep = { false, false, false, false, false, false, false, false, false, false, false, false }
local nk = 0
local function f(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = i + 1 }
    if i % 400 == 0 then nk = nk + 1 keep[nk % 12 + 1] = v end
    s = s + v.x + v.y
  end
  return s
end
f(100)
local c0 = collectgarbage("count")
local r = f(5000)
local c1 = collectgarbage("count")
print(r, nk, string.format("%.3f", c1 - c0))
"#,
    },
    Prog {
        name: "sink_count",
        small: false,
        src: r#"
-- many short-lived sunk tables: results stay exact and the collector's byte count stays sane
collectgarbage("stop")
local c0 = collectgarbage("count")
local function f(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = i + 1, z = i + 2 }
    s = s + v.x + v.y + v.z
  end
  return s
end
local r = 0
for k = 1, 50 do r = r + f(2000) end
collectgarbage("restart")
collectgarbage()
local c1 = collectgarbage("count")
print(r, math.abs(c1 - c0) < 200)
"#,
    },
    Prog {
        name: "sink_mem_error",
        small: true,
        src: r#"
-- F8 (TR29.3): with an injected host allocation failure while an exit materialises (tests/tr_core.rs), exactly one call raises
-- "not enough memory" and every other call is exact; without the injection nothing fails
local keep = {}
local function f(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = i * 0.5, flag = i > 3 }
    v.x = v.x + 1
    if i % 9 == 0 then keep[#keep % 5 + 1] = v end
    s = s + v.x + v.y
  end
  return s
end
local function want(n)
  local s = 0
  for i = 1, n do s = s + (i + 1) + i * 0.5 end
  return s
end
local okall, nfail, msg = true, 0, nil
for k = 1, 40 do
  local n = 30 + k
  local ok, e = pcall(f, n)
  if ok then
    if e ~= want(n) then okall = false end
  else
    nfail = nfail + 1
    msg = e
  end
end
print(okall, nfail, msg)
"#,
    },
    Prog {
        name: "j1_sinkf_mem_error",
        small: true,
        src: r#"
-- J1-2: as `sink_mem_error`, for the exits inside an inlined callee (the frameful inline stub): with an injected host allocation
-- failure exactly one call raises "not enough memory" and every other call is exact; the frames of the failing exit are rebuilt
-- (the error unwinds through them)
local keep = {}
local function norm(v, k)
  if k % 7 == 0 then return v.x * 2 end
  return v.x + v.y
end
local function f(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = i * 0.5 }
    s = s + norm(v, i)
    if i % 9 == 0 then keep[#keep % 5 + 1] = v end
  end
  return s
end
local function want(n)
  local s = 0
  for i = 1, n do
    if i % 7 == 0 then s = s + i * 2 else s = s + i + i * 0.5 end
  end
  return s
end
local okall, nfail, msg = true, 0, nil
for k = 1, 40 do
  local n = 30 + k
  local ok, e = pcall(f, n)
  if ok then
    if e ~= want(n) then okall = false end
  else
    nfail = nfail + 1
    msg = e
  end
end
print(okall, nfail, msg)
"#,
    },
    Prog {
        name: "j1_sinkd",
        small: true,
        src: r#"
-- J1-2 (H3-G): a sunk table that the loop's exits materialise over and over (a branch inside the inlined callee, every 4th iteration):
-- with `sinkdemote=N` the shape is demoted after N materialisations (the family is re-recorded without sinking it): the result stays exact
-- and the materialisations stop
local function norm(v, k)
  if k % 4 == 0 then return v.x * 2 end
  return v.x + v.y
end
local function f(n)
  local acc = 0
  for i = 1, n do
    local v = { x = i, y = i * 0.5 }
    acc = acc + norm(v, i)
  end
  return acc
end
local a = 0
for r = 1, 12 do a = a + f(3000 + r) end
print(a)
"#,
    },
    Prog {
        name: "sink_pacing",
        small: false,
        src: r#"
-- F8 (TR29.4): the virtual bytes of sunk tables are credited back at the cycle ends, so the collector keeps its pace: the
-- count stays within a small multiple of what the interpreter's own cycles leave (the 5.4 incremental credit was missing once:
-- the count grew to megabytes while the interpreter's stayed at ~100 KB). The outer loop allocates a string per round: that is
-- the safepoint where the collector steps (a loop that allocates nothing else never steps, its count only grows: no in-trace
-- step guard, JIT_TRACE_REF.md F8)
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
local res = {}
for _, mode in ipairs { "incremental", "generational" } do
  collectgarbage(mode)
  collectgarbage()
  local max, total = 0, 0
  for k = 1, 150 do
    total = total + work(40 + (k * 37) % 200, k)
    local str = string.format("%d", total)
    local c = collectgarbage("count")
    max = math.max(max, c) -- (no branch on a GC-dependent value: the instruction count must not depend on the collector)
  end
  res[#res + 1] = tostring(max < 1500) .. " " .. total
end
collectgarbage("incremental")
print(table.concat(res, " | "))
"#,
    },
];
