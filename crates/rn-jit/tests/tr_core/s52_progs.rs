//! Section 52 programs (`sinkmm`: the sunk-frame stub for exits inside metamethod frames; `resink`: sunk allocations replayed into side traces,
//! docs/JIT_TRACE.md section 52). Constructors run as metamethods (`__call`, arithmetic metamethods) inlined into loop traces, so that the exits with
//! sunk tables sit inside metamethod frames and get side traces; the side traces read, pass on, store or let escape the tables the parent had sunk.

use super::progs::Prog;

pub const S52_PROGS: &[Prog] = &[Prog {
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
}];
