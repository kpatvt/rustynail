//! Section 59 programs (docs/JIT_TRACE.md section 59): the hoisted loop budget (`budhoist`) and the chunk poll (`pollk`). Call-free counted loops of
//! every length around the chunk sizes the tests use (4, 64), nested, left early (break, type change, hole), with an error inside, with count / line
//! hooks set while the traces run, inside coroutines that yield between the loops, and with counts near 2^64. Each prints values that expose a wrong
//! iteration count after an exit or a chunk end; the count identity (`fetches + insns`) and the count budgets expose a wrong charge or refund.

use super::progs::Prog;

pub const S59_PROGS: &[Prog] = &[
    Prog {
        name: "s59_nested",
        small: false,
        src: r#"
-- inner loops of lengths around the chunk sizes, entered many times from an outer loop
local t = {}
for j = 1, 300 do t[j] = (j % 7) * 0.5 end
local function run(n, m)
  local out = 0.0
  for i = 1, n do
    local a = 0.0
    for j = 1, m do a = a + t[j] * 0.5 end
    out = out + a
  end
  return out
end
local acc = {}
for _, m in ipairs({0, 1, 2, 3, 4, 5, 7, 62, 63, 64, 65, 66, 127, 128, 129, 200, 300}) do
  local s = 0.0
  for r = 1, 30 do s = s + run(5, m) end
  acc[#acc + 1] = string.format("%d:%.2f", m, s)
end
-- integer loops with steps
local function isum(a, b, st)
  local s = 0
  for k = a, b, st do s = s + k end
  return s
end
for r = 1, 40 do isum(1, 100, 3) end
acc[#acc + 1] = isum(1, 100, 3) .. "/" .. isum(100, 1, -7) .. "/" .. isum(5, 4, 1)
print(table.concat(acc, " "))
"#,
    },
    Prog {
        name: "s59_break",
        small: false,
        src: r#"
-- body exits at every position of a chunk: break, a type change, a hole, a bound
local function find(t, n, x)
  local s = 0
  for j = 1, n do
    local v = t[j]
    if v > x then break end
    s = s + v
  end
  return s
end
local function sum(t, n)
  local s = 0.0
  for j = 1, n do s = s + t[j] end
  return s
end
local t = {}
for j = 1, 200 do t[j] = j end
local out = {}
for r = 1, 50 do find(t, 200, 1000) end
for _, x in ipairs({0, 1, 3, 4, 5, 63, 64, 65, 66, 150, 199, 1000}) do out[#out + 1] = find(t, 200, x) end
local f = {}
for j = 1, 130 do f[j] = j * 0.5 end
for r = 1, 50 do sum(f, 130) end
for _, k in ipairs({1, 4, 5, 64, 65, 66, 130}) do
  local g = {}
  for j = 1, 130 do g[j] = j * 0.5 end
  g[k] = "x"
  out[#out + 1] = tostring(pcall(sum, g, 130))
  g[k] = 1.25
  out[#out + 1] = sum(g, 130)
  -- an integer element: the body exits at a type guard and the interpreter finishes the loop with the count of the snapshot
  g[k] = 7
  out[#out + 1] = sum(g, 130)
  g[k + 1] = 9
  out[#out + 1] = sum(g, 130)
end
-- a long loop (many chunks) with integer elements in later chunks
local h = {}
for j = 1, 400 do h[j] = j * 0.5 end
for r = 1, 20 do sum(h, 400) end
h[70] = 3; h[200] = 5; h[399] = 11
out[#out + 1] = sum(h, 400)
-- reads past the end (a bound exit) at the last iteration of a chunk
out[#out + 1] = tostring(pcall(sum, f, 132))
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "s59_error",
        small: false,
        src: r#"
-- an error raised by the instruction behind a body exit, caught, then the loop entered again
local function acc(t, n)
  local s = 0
  for j = 1, n do s = s + t[j] * 2 end
  return s
end
local t = {}
for j = 1, 100 do t[j] = j end
for r = 1, 60 do acc(t, 100) end
local out = {}
for _, k in ipairs({1, 2, 4, 5, 63, 64, 65, 100}) do
  local old = t[k]
  t[k] = {}
  local ok, e = pcall(acc, t, 100)
  out[#out + 1] = tostring(ok) .. ":" .. tostring(e):gsub("^.-:%d+: ", "")
  t[k] = old
  out[#out + 1] = acc(t, 100)
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "s59_hooks",
        small: false,
        src: r#"
-- count and line hooks set while the loop's trace runs (from an outer iteration), every count small; the hook records where it fired
local t = {}
for j = 1, 100 do t[j] = j end
local function work(n)
  local s = 0
  for j = 1, n do s = s + t[j] end
  return s
end
local log = {}
local function hook(ev, line)
  local info = debug.getinfo(2, "l")
  local j = "-"
  for k = 1, 20 do
    local name, v = debug.getlocal(2, k)
    if name == nil then break end
    if name == "j" or name == "s" then j = j .. name .. "=" .. tostring(v) end
  end
  log[#log + 1] = ev:sub(1, 1) .. (line or info.currentline) .. "/" .. j
end
local total = 0
for r = 1, 200 do
  if r == 120 then debug.sethook(hook, "", 3) end
  if r == 122 then debug.sethook(hook, "", 1) end
  if r == 123 then debug.sethook(hook, "", 17) end
  if r == 126 then debug.sethook(hook, "l") end
  if r == 127 then debug.sethook(hook, "l", 5) end
  if r == 128 then debug.sethook() end
  total = total + work(70)
end
debug.sethook()
print(total, #log, table.concat(log, " ", 1, math.min(#log, 400)))
"#,
    },
    Prog {
        name: "s59_coro",
        small: false,
        src: r#"
-- counted loops inside coroutines that yield between them (and from a hook-free helper inside the outer loop)
local t = {}
for j = 1, 80 do t[j] = j * 0.25 end
local function gen(m)
  return coroutine.wrap(function()
    for i = 1, 40 do
      local a = 0.0
      for j = 1, m do a = a + t[j] end
      coroutine.yield(a)
    end
    return -1
  end)
end
local out = {}
for _, m in ipairs({1, 3, 4, 5, 64, 65, 80}) do
  local g = gen(m)
  local s = 0.0
  for k = 1, 40 do s = s + g() end
  out[#out + 1] = string.format("%.2f", s)
end
-- two coroutines interleaved, each in its own loop
local a, b = gen(70), gen(5)
local s = 0.0
for k = 1, 40 do s = s + a() - b() end
out[#out + 1] = string.format("%.2f", s)
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "s59_counts",
        small: false,
        src: r#"
-- loop counts near 2^64 (the chunk bias wraps), left by a break, and long loops spanning many chunks
local function upto(a, b, st, lim)
  local s, c = 0, 0
  for k = a, b, st do
    c = c + 1
    if c > lim then break end
    s = s + (k & 7)
  end
  return s, c
end
local out = {}
for r = 1, 30 do upto(1, 1000, 1, 2000) end
for _, x in ipairs({
  {math.mininteger, math.maxinteger, 1, 300},
  {math.maxinteger - 70, math.maxinteger, 1, 1000},
  {math.mininteger, math.mininteger + 70, 1, 1000},
  {math.maxinteger, math.mininteger, -1, 130},
  {0, math.maxinteger, 2, 65},
  {1, 100000, 1, 1000000},
}) do
  local s, c = upto(x[1], x[2], x[3], x[4])
  out[#out + 1] = s .. "/" .. c
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "s63_policy",
        small: false,
        src: r#"
-- section 63 (`bhmin`): loops whose root recording sees few iterations left keep the per-iteration body, long ones are hoisted; each phase then
-- runs the other kind of trip (a short-recorded loop run long, a long-recorded loop left early), so both forms meet every count and exit
local free = {}
for c = 1, 8 do free[c] = (c % 3 == 0) end
local hits = 0
local function hit(c) hits = hits + c end
-- the queens shape: 8 iterations, mostly left early through a guard into the call path
local function short(n)
  local s = 0
  for c = 1, n do
    if free[c] then hit(c) else s = s + c end
  end
  return s
end
local t = {}
for j = 1, 2000 do t[j] = j % 11 end
-- a long call-free loop (recorded with many iterations left)
local function long(m)
  local s = 0
  for j = 1, m do s = s + t[j] end
  return s
end
-- a loop recorded short (m = 5 while it gets hot), later run long
local function grow(m)
  local s = 0
  for j = 1, m do s = s + t[j] * 2 end
  return s
end
local out = {}
local a = 0
for r = 1, 400 do a = a + short(8) end
out[#out + 1] = a .. "/" .. hits
local b = 0
for r = 1, 60 do b = b + long(1500) end
-- long loop left early: a hole, then a type change in the middle of a chunk
t[700] = nil
local ok1, e1 = pcall(long, 1500)
t[700] = 700 % 11
t[1300] = 2.5
b = b + long(1500) + long(40) + long(1)
t[1300] = 1300 % 11
out[#out + 1] = b .. "/" .. tostring(ok1) .. "/" .. tostring(e1 and e1:match("arith") ~= nil)
local g = 0
for r = 1, 400 do g = g + grow(5) end
for r = 1, 20 do g = g + grow(1999) end
t[1000] = "x"
local ok2 = pcall(grow, 1999)
t[1000] = 1000 % 11
out[#out + 1] = g .. "/" .. tostring(ok2)
print(table.concat(out, " "))
"#,
    },
];
