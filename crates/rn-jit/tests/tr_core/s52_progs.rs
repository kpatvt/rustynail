//! Section 52 programs (`sinkmm`: the sunk-frame stub for exits inside metamethod frames; `resink`: sunk allocations replayed into side traces,
//! docs/JIT_TRACE.md section 52). Constructors run as metamethods (`__call`, arithmetic metamethods) inlined into loop traces, so that the exits with
//! sunk tables sit inside metamethod frames and get side traces; the side traces read, pass on, store or let escape the tables the parent had sunk.

use super::progs::Prog;

pub const S52_PROGS: &[Prog] = &[
    Prog {
        name: "s52_mm_frames",
        small: false,
        src: r#"
-- classic-style classes: `Class(...)` is `__call`, the vector operators are metamethods that call the constructor: every temporary is a sunk
-- table made inside a metamethod frame; receivers of two classes make the guards inside those frames fail, so side traces hang off exits whose
-- snapshots hold metamethod frames and sunk tables. Sections: plain rounds, an error raised inside a metamethod behind such an exit, a traceback
-- taken from inside one, a count hook installed between rounds, a coroutine yielding inside the loop, and a method replaced between rounds
-- (its side traces are flushed while the parents live).
local Object = {}
Object.__index = Object
function Object:extend()
  local cls = {}
  for k, v in pairs(self) do if k:find("__") == 1 then cls[k] = v end end
  cls.__index = cls
  cls.super = self
  return setmetatable(cls, self)
end
function Object:__call(...)
  local o = setmetatable({}, self)
  o:new(...)
  return o
end
local Vec = Object:extend()
function Vec:new(x, y) self.x = x or 0; self.y = y or 0 end
function Vec.__add(a, b) return Vec(a.x + b.x, a.y + b.y) end
function Vec.__sub(a, b) return Vec(a.x - b.x, a.y - b.y) end
function Vec.__mul(a, b)
  if type(a) == "number" then return Vec(a * b.x, a * b.y)
  elseif type(b) == "number" then return Vec(a.x * b, a.y * b) end
  return a.x * b.x + a.y * b.y
end
function Vec.__unm(a) return Vec(-a.x, -a.y) end
local P = Object:extend()
function P:new(x, y) self.pos = Vec(x, y); self.force = Vec(0, 0) end
function P:push(f) self.force = self.force + f end
local H = P:extend()
function H:push(f) self.force = self.force + f * 2 end
local trace_at = -1
local tb
local raise_at = -1
function P:check(f)
  if f.x == raise_at then error("raised " .. f.y) end
  if f.x == trace_at then tb = debug.traceback("tb", 1):gsub("0x%x+", "ADDR") end
  self.force = self.force + f
end
H.check = P.check
local ps = {}
for i = 1, 20 do ps[i] = ((i % 3 == 0) and H or P)(i, i * 2) end
local function round(n, k0)
  for k = 1, n do
    local a, b = ps[(k + k0) % 20 + 1], ps[(k * 7 + k0) % 20 + 1]
    local d = b.pos - a.pos
    local f = d * 0.5
    a:push(f)
    b:push(-f)
  end
end
local function round2(n)
  for k = 1, n do
    local a = ps[k % 20 + 1]
    local f = Vec(k % 7, k % 5) + Vec(1, 2)
    a:check(f)
  end
end
local function sum()
  local sx, sy = 0, 0
  for i = 1, 20 do sx = sx + ps[i].force.x; sy = sy + ps[i].force.y end
  return string.format("%.4f %.4f", sx, sy)
end
for r = 1, 12 do round(150, r) end
print("plain", sum())
for r = 1, 6 do round2(150) end
print("check", sum())
raise_at = 5
print("raise", pcall(round2, 150))
raise_at = -1
print("after", sum())
trace_at = 3
round2(60)
trace_at = -1
print("tb", tb and #tb > 0)
local hooks = 0
debug.sethook(function() hooks = hooks + 1 end, "", 1000)
for r = 1, 3 do round(150, r) end
debug.sethook()
print("hooked", sum(), hooks > 0)
local co = coroutine.wrap(function()
  for r = 1, 6 do round(100, r); coroutine.yield(sum()) end
  return "done"
end)
for i = 1, 7 do print("co", co()) end
-- a method replaced while traces through it live: their side traces are flushed under live parents
for r = 1, 4 do
  function H:push(f) self.force = self.force + f * (r + 1) end
  round(150, r)
  print("swap", r, sum())
end
"#,
    },
    Prog {
        name: "s52_resink_paths",
        small: false,
        src: r#"
-- sunk temporaries live at exits that get side traces; the side traces read them (re-sunk), pass them on to metamethods (re-sunk), or let them escape
-- on some paths (into an array, a weak table, an upvalue, a global): the replay is then reverted. Field values of every kind (integers, floats,
-- booleans, short strings, functions, tables), two slots holding the same table, two tables in one exit, and a branch inside the side traces that
-- flips late (side traces of side traces).
local V = {}
V.__index = V
local function vec(x, y) return setmetatable({ x = x, y = y }, V) end
V.__add = function(a, b) return vec(a.x + b.x, a.y + b.y) end
V.__unm = function(a) return vec(-a.x, -a.y) end
local P = {}; P.__index = P
local Q = setmetatable({}, { __index = P }); Q.__index = Q
function P:take(f) self.s = self.s + f.x * 2 + f.y end
function Q:take(f) self.s = self.s + f.x - f.y end
local kept = {}
local weak = setmetatable({}, { __mode = "k" })
local last
function P:grab(f, k) if k % 13 == 0 then kept[#kept + 1] = f end; self.s = self.s + f.x end
function Q:grab(f, k) if k % 17 == 0 then weak[f] = k end; if k % 19 == 0 then last = f end; self.s = self.s + f.y end
local objs = {}
local function reset() for i = 1, 16 do objs[i] = setmetatable({ s = 0 }, (i % 4 == 0) and Q or P) end end
local function total() local t = 0; for i = 1, 16 do t = t + objs[i].s end; return t end
local fn = function(a) return a + 1 end
local shared = { 1, 2, 3 }
local function mixed(n)
  local acc = 0
  for k = 1, n do
    local r = { i = k, f = k * 0.5, b = (k % 2 == 0), s = "s" .. (k % 3), fn = fn, t = shared }
    local alias = r
    local o = objs[k % 16 + 1]
    o:take({ x = r.i, y = alias.f })
    if r.b then acc = acc + r.fn(r.i) + #r.t else acc = acc + #r.s end
  end
  return acc
end
local function sums(n, flip)
  for k = 1, n do
    local f = vec(k % 7, k % 5)
    local g = -f
    local o = objs[(k * 3) % 16 + 1]
    o:take(f + g)
    if k > flip and k % 3 == 0 then o:take(f) end
  end
end
local function escapes(n)
  for k = 1, n do
    local f = vec(k % 11, k % 4)
    objs[k % 16 + 1]:grab(f, k)
  end
end
reset()
for r = 1, 8 do print("mixed", r, mixed(200)) end
reset()
for r = 1, 8 do sums(200, r < 5 and 1000 or 50) end
print("sums", total())
reset()
for r = 1, 6 do escapes(200) end
collectgarbage()
local nw = 0
for _ in pairs(weak) do nw = nw + 1 end
print("escapes", total(), #kept, kept[3] and kept[3].x, getmetatable(kept[5]) == V, last and last.x, nw >= 0)
-- a materialised replayed table keeps the layout the interpreter builds: `pairs` order and `next`
local seen = {}
for k, v in pairs(kept[7]) do seen[#seen + 1] = k .. "=" .. tostring(v) end
print("layout", table.concat(seen, ","))
"#,
    },
    Prog {
        name: "s52_resink_count",
        small: false,
        src: r#"
-- with the collector stopped, the bytes of a loop whose temporaries are re-sunk in side traces (virtual in the root, replayed in the side
-- traces, materialised at their exits) equal the interpreter's
collectgarbage("stop")
local V = {}; V.__index = V
local function vec(x, y) return setmetatable({ x = x, y = y }, V) end
V.__add = function(a, b) return vec(a.x + b.x, a.y + b.y) end
local A = {}; A.__index = A
local B = setmetatable({}, { __index = A }); B.__index = B
local keep = {}
function A:put(f, k) self.s = self.s + f.x end
function B:put(f, k) if k % 50 == 0 then keep[#keep + 1] = f end; self.s = self.s + f.y end
local objs = {}
for i = 1, 8 do objs[i] = setmetatable({ s = 0 }, (i % 3 == 0) and B or A) end
local function run(n)
  for k = 1, n do
    local f = vec(k % 7, 1) + vec(1, k % 3)
    objs[k % 8 + 1]:put(f, k)
  end
end
run(300)
local c0 = collectgarbage("count")
run(3000)
local c1 = collectgarbage("count")
local s = 0
for i = 1, 8 do s = s + objs[i].s end
print(s, #keep, string.format("%.3f", c1 - c0))
"#,
    },
    Prog {
        name: "s52_resink_observers",
        small: false,
        src: r#"
-- the re-sunk tables are observed from the side traces' exits: an error with the table live (`debug.getlocal` reads it), a hook installed from
-- inside, a coroutine that yields while the table is live, `__gc` added to the metatable late (finalizable tables are never sunk), `__newindex`
-- and `__index` on the metatable, collections in the loop, and a callee replaced between rounds (its side traces are flushed under live parents)
local V = {}
V.__index = function(t, k) return k == "z" and 0 or nil end
local function vec(x, y) return setmetatable({ x = x, y = y }, V) end
V.__add = function(a, b) return vec(a.x + b.x, a.y + b.y + a.z) end
local A = {}; A.__index = A
local B = setmetatable({}, { __index = A }); B.__index = B
function A:use(f, k) self.s = self.s + f.x + f.y end
function B:use(f, k) self.s = self.s + f.x * 3 end
local objs = {}
for i = 1, 12 do objs[i] = setmetatable({ s = 0 }, (i % 3 == 0) and B or A) end
local function total() local t = 0; for i = 1, 12 do t = t + objs[i].s end; return t end
local bad = -1
function A:check(f, k)
  if k == bad then
    local name, val = debug.getlocal(1, 2)
    error("bad " .. name .. " " .. val.x .. " " .. val.y .. " " .. tostring(getmetatable(val) == V))
  end
  self.s = self.s + f.y
end
B.check = A.check
local function run(n)
  for k = 1, n do
    local f = vec(k % 9, k % 4) + vec(1, 1)
    objs[k % 12 + 1]:use(f, k)
    objs[(k + 5) % 12 + 1]:check(f, k)
    if k % 97 == 0 then collectgarbage("step") end
  end
end
for r = 1, 6 do run(240) end
print("plain", total())
bad = 77
print("err", pcall(run, 240))
bad = -1
print("after", total())
local hooks = 0
local function hooked(n)
  for k = 1, n do
    local f = vec(k % 5, 2) + vec(k % 3, 1)
    objs[k % 12 + 1]:use(f, k)
    if k == 100 then debug.sethook(function() hooks = hooks + 1 end, "", 50) end
  end
  debug.sethook()
end
hooked(240)
print("hook", total(), hooks > 0)
local co = coroutine.wrap(function()
  for r = 1, 5 do
    for k = 1, 120 do
      local f = vec(k % 6, r) + vec(1, 0)
      objs[k % 12 + 1]:use(f, k)
      if k % 40 == 0 then coroutine.yield(f.x + f.y) end
    end
  end
  return -1
end)
local ys = 0
for i = 1, 20 do local y = co(); ys = ys + y; if y < 0 then break end end
print("co", ys, total())
for r = 1, 3 do
  -- (a fresh prototype per round: the old one is collected, the side traces that inlined it are flushed while their parent lives, and the parent's
  -- exits land in tier 1 again with the replay cell stale)
  B.use = load("return function(self, f, k) self.s = self.s + f.x * " .. (r + 2) .. " end")()
  collectgarbage()
  collectgarbage()
  run(200)
  print("swap", r, total())
end
local fin = 0
collectgarbage("stop")
V.__gc = function() fin = fin + 1 end
run(120)
collectgarbage("restart")
collectgarbage()
collectgarbage()
print("gc", total(), fin > 0)
"#,
    },
];
