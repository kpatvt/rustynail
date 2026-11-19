//! A1 programs (side traces inherit the types of the parent's unmodified `SLOAD`s; exit and writeback tag stores of a slot whose
//! stack tag is provably the exit value's static tag are skipped). Each program keeps its loops inside functions and prints
//! values that expose a wrong stack tag or a wrongly unchecked load (`type`, `math.type`, the value itself).

use super::progs::Prog;

pub const A1_PROGS: &[Prog] = &[
    Prog {
        name: "a1_poly_later_sload",
        small: false,
        src: r#"
-- the cold exit (`i > 50`) sits before the first read of `acc`: a side trace from it must check the type of `acc`
local function f(a, b, acc)
  for i = a, b do
    if i > 50 then acc = acc + 1 else acc = acc * 2 + i end
  end
  return acc
end
local out = {}
for k = 1, 60 do f(1, 40, 0) end
for k = 1, 60 do f(51, 70, 0) end
for k = 1, 5 do out[#out + 1] = tostring(f(51, 70, 0)) end
for k = 1, 30 do f(51, 60, 0.5) end
for k = 1, 5 do out[#out + 1] = tostring(f(51, 60, 0.5)) end
for k = 1, 5 do out[#out + 1] = tostring(f(1, 60, 0.25)) end
for k = 1, 5 do out[#out + 1] = tostring(f(45, 55, 3)) end
for k = 1, 5 do out[#out + 1] = tostring(f(60, 55, 3)) end
print(table.concat(out, " "))
-- the same with a string and an integer-valued float as the accumulator
local r = {}
for _, v in ipairs { 1, 1.0, "2", 2.5, 7 } do
  local ok, e = pcall(f, 51, 60, v)
  r[#r + 1] = tostring(ok) .. ":" .. tostring(e) .. ":" .. (math.type(e) or type(e))
end
print(table.concat(r, " "))
"#,
    },
    Prog {
        name: "a1_type_change",
        small: false,
        src: r#"
-- a slot that holds one type at the trace entry and another at an early exit: the exit must write the tag of the new value
local function fi(n, x, lim) -- float -> int
  local s = 0.0
  for i = 1, n do
    s = s + x
    x = i
    if i == lim then return s, x, math.type(x) end
    x = x + 0.5
  end
  return s, x, math.type(x)
end
local function iff(n, x, lim) -- int -> float
  local s = 0
  for i = 1, n do
    s = s + x
    x = i + 0.5
    if i == lim then return s, x, math.type(x) end
    x = i
  end
  return s, x, math.type(x)
end
local function inl(n, x, lim) -- int -> nil
  local s = 0
  for i = 1, n do
    s = s + x
    x = nil
    if i == lim then return s, x, type(x) end
    x = i
  end
  return s, x, type(x)
end
local function ist(n, x, lim) -- int -> string
  local s = 0
  for i = 1, n do
    s = s + x
    x = "abc"
    if i == lim then return s, x, type(x) end
    x = i
  end
  return s, x, type(x)
end
local function itb(n, x, lim) -- int -> table
  local s = 0
  for i = 1, n do
    s = s + x
    x = {}
    if i == lim then return s, type(x), #x end
    x = i
  end
  return s, type(x)
end
local function bool(n, x, lim) -- int -> true -> false
  local s = 0
  for i = 1, n do
    s = s + x
    x = true
    if i == lim then return s, x, type(x) end
    x = false
    if i == lim + 1 then return s, x, type(x) end
    x = i
  end
  return s, x, type(x)
end
local function show(...)
  local t = table.pack(...)
  for i = 1, t.n do t[i] = tostring(t[i]) end
  return table.concat(t, "/")
end
local out = {}
for _, spec in ipairs { { fi, 0.5 }, { iff, 3 }, { inl, 3 }, { ist, 3 }, { itb, 3 }, { bool, 3 } } do
  local f, x = spec[1], spec[2]
  for k = 1, 50 do f(30, x, 1000) end
  for k = 1, 50 do f(30, x, 7) end
  for k = 1, 4 do out[#out + 1] = show(f(30, x, 7)) end
  out[#out + 1] = show(f(30, x, 29))
  out[#out + 1] = show(f(30, x, 1))
  out[#out + 1] = show(f(30, x, 1000))
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "a1_flush_rerecord",
        small: false,
        src: r#"
-- the root trace is flushed and re-recorded with other types while its side traces live on
local function g(t, n, acc)
  for i = 1, n do
    local d = t[i]
    if d > 0 then acc = acc + d else acc = acc - 1 end
  end
  return acc
end
local ti, tf = {}, {}
for i = 1, 60 do
  ti[i] = (i % 7 == 0) and -i or i
  tf[i] = (i % 7 == 0) and (-i - 0.5) or (i + 0.5)
end
local out = {}
for round = 1, 3 do
  for k = 1, 40 do g(ti, 60, 0) end
  out[#out + 1] = tostring(g(ti, 60, 0))
  for k = 1, 40 do g(tf, 60, 0.0) end
  out[#out + 1] = tostring(g(tf, 60, 0.0))
  for k = 1, 40 do g(ti, 60, 0.5) end
  out[#out + 1] = tostring(g(ti, 60, 0.5))
  for k = 1, 40 do g(tf, 60, 0) end
  out[#out + 1] = tostring(g(tf, 60, 0))
  out[#out + 1] = tostring(g(ti, 7, 0)) .. "|" .. tostring(g(tf, 7, 0))
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "a1_nested_trips",
        small: false,
        src: r#"
-- an nbody-like pair loop whose inner trip count varies (4, 3, 2, 1, 0)
local function adv(b, n)
  for i = 1, n do
    local bi = b[i]
    local x, v = bi.x, bi.v
    for j = i + 1, n do
      local bj = b[j]
      local dx = x - bj.x
      v = v - dx * 0.5
      bj.v = bj.v + dx * 0.25
    end
    bi.v = v
    bi.x = bi.x + v * 0.01
  end
end
local function mk(n)
  local b = {}
  for i = 1, n do b[i] = { x = i * 1.5, v = 0.25 * i } end
  return b
end
local function sum(b)
  local s = 0.0
  for i = 1, #b do s = s + b[i].x * 3 + b[i].v end
  return s
end
local out = {}
for _, n in ipairs { 5, 3, 6, 1, 0, 5, 2, 4, 5 } do
  local b = mk(n)
  for k = 1, 60 do adv(b, n) end
  out[#out + 1] = string.format("%d:%.9g", n, sum(b))
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "a1_hooks",
        small: false,
        src: r#"
-- count / line hooks that change a local of the hot loop (integer <-> float) between iterations
local function body(n, x)
  local s = 0.0
  for i = 1, n do
    if i % 4 == 0 then s = s + x else s = s + 1.5 end
    x = x + 1
  end
  return s, x, math.type(x)
end
local calls = 0
local function flip(...)
  calls = calls + 1
  local k = 1
  while true do
    local name, val = debug.getlocal(2, k)
    if not name then return end
    if name == "x" then
      if calls % 3 == 0 then debug.setlocal(2, k, val + 0.5) elseif calls % 3 == 1 then debug.setlocal(2, k, math.tointeger(val) or 1) end
      return
    end
    k = k + 1
  end
end
local out = {}
for k = 1, 30 do body(40, 1) end
for _, mode in ipairs { "count", "line" } do
  calls = 0
  if mode == "count" then debug.sethook(flip, "", 37) else debug.sethook(flip, "l") end
  for k = 1, 3 do
    local s, x, t = body(40, 1)
    out[#out + 1] = string.format("%s %.9g %.9g %s", mode, s, x, t)
  end
  debug.sethook()
  out[#out + 1] = mode .. " calls " .. calls
  for k = 1, 30 do body(40, 1) end
  local s, x, t = body(40, 1)
  out[#out + 1] = string.format("%.9g %.9g %s", s, x, t)
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "a1_coroutines",
        small: false,
        src: r#"
-- coroutines that yield from the loop; one of them is fed a float where the others get integers
local function gen(n, x, lim)
  return coroutine.wrap(function()
    local s = 0
    for i = 1, n do
      if i > lim then s = s + 1 else s = s * 2 + x end
      if i % 5 == 0 then coroutine.yield(s) end
    end
    return s
  end)
end
local out = {}
for k = 1, 30 do
  local g = gen(25, 1, 20)
  local last
  for _ = 1, 6 do last = g() end
  if k % 10 == 0 then out[#out + 1] = tostring(last) end
end
for _, x in ipairs { 1, 0.5, 2, 1.5 } do
  local g = gen(25, x, 20)
  local t = {}
  for _ = 1, 6 do t[#t + 1] = tostring(g()) end
  out[#out + 1] = table.concat(t, ",")
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "a1_gc_error",
        small: false,
        src: r#"
-- allocation (GC pressure) and errors from inside the hot loop
local function h(n, acc, bad)
  local keep = {}
  for i = 1, n do
    if i == bad then acc = acc .. "x" end
    local t = { i, acc }
    keep[#keep % 7 + 1] = t
    acc = acc + i
    if i % 16 == 0 then collectgarbage("step", 1) end
  end
  return acc, #keep
end
local out = {}
for k = 1, 40 do h(50, 0, 1000) end
for _, bad in ipairs { 1000, 3, 50, 1 } do
  local ok, a, b = pcall(h, 50, 0, bad)
  out[#out + 1] = tostring(ok) .. ":" .. tostring(a):gsub("^.-:%d+: ", "") .. ":" .. tostring(b)
end
for k = 1, 30 do h(50, 0.5, 1000) end
local ok, a, b = pcall(h, 50, 0.5, 20)
out[#out + 1] = tostring(ok) .. ":" .. tostring(a):gsub("^.-:%d+: ", "")
local ok2, a2, b2 = pcall(h, 50, 0, 1000)
out[#out + 1] = tostring(ok2) .. ":" .. tostring(a2) .. ":" .. tostring(b2)
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "a1_prelude_known",
        small: false,
        src: r#"
-- the prelude of a loop head is entered through the head's dispatch cell, not through the exit it was recorded from:
-- it must check every slot it reads, also those its parent's earlier entry checks proved for the parent's own entry
local function f(s, t, n)
  for i = 1, n do s = s + t end
  return s
end
local out = {}
for k = 1, 10 do f(1, 2, 100) end
for k = 1, 200 do f(1, 2.5, 100) end
for k = 1, 3 do out[#out + 1] = tostring(f(1.5, 2.5, 100)) end
for k = 1, 3 do out[#out + 1] = tostring(f(2^40, 0.5, 100)) end
for k = 1, 3 do out[#out + 1] = tostring(f(3, 2.5, 100)) end
for k = 1, 3 do out[#out + 1] = tostring(f(-0.0, 1, 100)) end
for k = 1, 3 do out[#out + 1] = tostring(pcall(f, "7", 2.5, 100)) end
print(table.concat(out, " "))
"#,
    },
    // ---- G1 (`guarddiet`): the chain's entry state is inherited through side traces; links skip the room checks of a guard-free entry ----
    Prog {
        name: "g1_nested_types",
        small: false,
        src: r#"
-- an nbody-like nest: inner trip counts vary, the slots the side traces never touch (x, y, z) change type between activations
local function f(n, x, y, z)
  local s, t = 0, 0.5
  for i = 1, n do
    local m = (i % 4) + 1
    for j = 1, m do s = s + x * j; t = t + y end
    s = s + z
  end
  return s, t, x, y, z
end
local out = {}
for k = 1, 40 do f(12, 1, 2, 3) end
for k = 1, 3 do out[#out + 1] = table.concat({ f(12, 1, 2, 3) }, ",") end
for k = 1, 30 do f(12, 1.5, 2, 3) end
for k = 1, 3 do out[#out + 1] = table.concat({ f(12, 1.5, 2, 3) }, ",") end
for k = 1, 30 do f(12, 1, 2.5, 3.5) end
for k = 1, 3 do out[#out + 1] = table.concat({ f(12, 1, 2.5, 3.5) }, ",") end
for k = 1, 3 do out[#out + 1] = table.concat({ f(12, 1, 2, 3.5) }, ",") end
for _, v in ipairs { "4", 4.0, 7, "0x10" } do
  local ok, a, b = pcall(f, 12, 1, 2, v)
  out[#out + 1] = tostring(ok) .. ":" .. tostring(a) .. ":" .. tostring(b)
  ok, a, b = pcall(f, 12, v, 2, 3)
  out[#out + 1] = tostring(ok) .. ":" .. tostring(a) .. ":" .. tostring(b)
end
local ok, e = pcall(f, 12, 1, nil, 3)
out[#out + 1] = tostring(ok) .. ":" .. (tostring(e):gsub("^.-:%d+: ", ""))
ok, e = pcall(f, 12, 1, 2, {})
out[#out + 1] = tostring(ok) .. ":" .. (tostring(e):gsub("^.-:%d+: ", ""))
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "g1_mid_store",
        small: false,
        src: r#"
-- a trace of the chain stores to a slot (k) that the root guards at its entry: the type guard must run again after the store
local function f(n, k, j0)
  local s = 0
  for i = 1, n do
    for j = 1, i % 4 + j0 do s = s + k end
    if i % 5 == 0 then k = k + 0.5 end
    if i % 7 == 0 then k = math.floor(k) end
    if i % 11 == 0 then k = tostring(k) end
    if i % 13 == 0 then k = tonumber(k) end
  end
  return s, k, math.type(k) or type(k)
end
local out = {}
for r = 1, 60 do f(30, 1, 1) end
for r = 1, 4 do out[#out + 1] = table.concat({ f(30, 1, 1) }, ",") end
for r = 1, 40 do f(30, 2.5, 1) end
for r = 1, 4 do out[#out + 1] = table.concat({ f(30, 2.5, 1) }, ",") end
for r = 1, 4 do out[#out + 1] = table.concat({ f(100, 3, 2) }, ",") end
for r = 1, 4 do out[#out + 1] = table.concat({ f(100, 3.5, 0) }, ",") end
print(table.concat(out, " "))
local function g(n, a, b)
  for i = 1, n do
    for j = 1, i % 3 + 1 do a = a + b end
    if i % 4 == 0 then b = b + 0.25 end
    if i % 9 == 0 then b = 1 end
    if i % 10 == 0 then a = a // 1 end
  end
  return a, b, math.type(a), math.type(b)
end
local r = {}
for k = 1, 50 do g(20, 0, 1) end
for k = 1, 3 do r[#r + 1] = table.concat({ g(20, 0, 1) }, ",") end
for k = 1, 3 do r[#r + 1] = table.concat({ g(20, 0.5, 2) }, ",") end
for k = 1, 3 do r[#r + 1] = table.concat({ g(37, 7, 1.5) }, ",") end
print(table.concat(r, " "))
"#,
    },
    Prog {
        name: "bk_lazy_link_root",
        small: false,
        src: r#"
-- a lazy-aware (loop-free, self-recursive) root whose inlined callee has a loop: with `nest=2` the root ends with a frame link, and the
-- region root must be stored at its entry like any root's (a stale `lazy_root` from an earlier, deeper descent walked off the CallInfo chain)
local function inner(n, x)
  local s = 0
  for i = 1, n do
    for j = 1, i % 3 + 1 do s = s + x end
    if i % 5 == 0 then s = s + 1 end
  end
  return s
end
local function rec(d, x)
  local s = inner(7, x)
  if d > 0 then s = s + rec(d - 1, x) end
  return s
end
local out = {}
for _, d in ipairs { 0, 1, 5, 50, 1, 0 } do
  for k = 1, 3 do out[#out + 1] = rec(d, 1) end
end
print(table.concat(out, " "))
-- the same shape called from C (pcall: a CIST_FRESH root frame) after a deep descent
for k = 1, 4 do
  out[#out + 1] = select(2, pcall(rec, 40 + k, 2))
  out[#out + 1] = select(2, pcall(rec, 2, 2))
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "g1_room_deep",
        small: false,
        src: r#"
-- loops with exits inside frames at growing depth (stack growth in a loop, the stack limit, the CallInfo list)
local function inner(n, x)
  local s = 0
  for i = 1, n do
    for j = 1, i % 3 + 1 do s = s + x end
    if i % 5 == 0 then s = s + 1 end
  end
  return s
end
local function rec(d, x)
  local s = inner(7, x)
  if d > 0 then s = s + rec(d - 1, x) end
  return s
end
local out = {}
for _, d in ipairs { 0, 1, 5, 50, 300, 800, 1, 0 } do
  for k = 1, 3 do out[#out + 1] = rec(d, 1) end
end
out[#out + 1] = rec(100, 1.5)
print(table.concat(out, " "))
local function grow(n, ...)
  local s = 0
  for i = 1, n do
    for j = 1, i % 3 + 1 do s = s + select('#', ...) end
  end
  return s
end
local t = {}
for k = 1, 40 do t[#t + 1] = k end
for k = 1, 5 do print(grow(30, table.unpack(t, 1, 5 * k))) end
"#,
    },
    Prog {
        name: "g1_hooks_coro_gc_err",
        small: false,
        src: r#"
-- the guarded nest under a count hook, in a coroutine that yields in the inner loop, with garbage and errors at varying iterations
local function f(n, x, bad)
  local s = 0
  for i = 1, n do
    for j = 1, i % 3 + 1 do
      s = s + x
      if i == bad and j == 2 then error("boom " .. i) end
    end
    local g = { i, tostring(i) }
    s = s + #g
  end
  return s
end
local out = {}
for k = 1, 40 do f(20, 1, 0) end
local fired = 0
debug.sethook(function() fired = fired + 1 end, "", 50)
for k = 1, 10 do out[#out + 1] = f(20, 1, 0) end
debug.sethook()
out[#out + 1] = fired > 0 and "hook" or "nohook"
for k = 1, 6 do
  local ok, e = pcall(f, 20, 1, k * 3)
  out[#out + 1] = tostring(ok) .. ":" .. (tostring(e):gsub("^.-:%d+: ", ""))
end
local co = coroutine.wrap(function(n, x)
  local s = 0
  for i = 1, n do
    for j = 1, i % 3 + 1 do s = s + x end
    if i % 4 == 0 then coroutine.yield(s) end
  end
  return -s
end)
for k = 1, 5 do out[#out + 1] = co(16, 2) end
local co2 = coroutine.wrap(function() for r = 1, 6 do coroutine.yield(f(20, 1.5, 0)) end end)
for k = 1, 6 do out[#out + 1] = co2() end
for k = 1, 20 do collectgarbage("step") out[#out + 1] = f(20, 2, 0) end
print(table.concat(out, " "))
"#,
    },
];

/// Programs that are too slow for the `+tracemove` / `stackmove` twins (the stack limit takes ~200000 frames): run only by `g1_guard_diet_programs`.
pub const G1_SLOW: &[Prog] = &[Prog {
    name: "g1_stack_limit",
    small: false,
    src: r#"
-- an endless recursion whose frames run a hot loop: the stack overflow error is caught and the traces keep running afterwards
local function inner(n, x)
  local s = 0
  for i = 1, n do
    for j = 1, i % 3 + 1 do s = s + x end
    if i % 5 == 0 then s = s + 1 end
  end
  return s
end
local function inf(d)
  local s = inner(5, 1)
  return s + inf(d + 1) + 1
end
for k = 1, 3 do
  local ok, e = pcall(inf, 0)
  print(ok, (tostring(e):find("stack overflow", 1, true)) ~= nil, inner(9, 2))
end
"#,
}];
