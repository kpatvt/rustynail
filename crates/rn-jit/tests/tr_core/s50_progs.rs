//! Section 50 programs (`slink`: register-passing side links for traces that are not lazy-aware, docs/JIT_TRACE.md section 50). Each has a loop root
//! trace (not a recursion family) whose exits get side traces: frameless exits and exits inside inlined callees (the side trace keeps the frames
//! virtual), with integers, floats, strings, tables and booleans live across the exit, more live values than registers (spill slots as sources), side
//! traces of side traces, and observers after the link: errors with `debug.getlocal`, count hooks installed from inside, collections, coroutine
//! yields between batches, a callee replaced between rounds (the side trace is flushed while its parent lives: the link is undone).

use super::progs::Prog;

pub const S50_PROGS: &[Prog] = &[
    Prog {
        name: "s50_frame_sides",
        small: false,
        src: r#"
-- a loop root that inlines small methods; branches inside the callees flip with the data, so exits inside inlined frames get side traces
local P = {}
P.__index = P
function P.new(x, y, name) return setmetatable({ x = x, y = y, name = name, hits = 0 }, P) end
function P:step(k, f)
  if k % 3 == 0 then
    self.x = self.x + f
  elseif k % 3 == 1 then
    self.y = self.y - f * 0.5
  else
    self.hits = self.hits + 1
  end
  if self.hits % 5 == 4 then return self.x * 2 end
  return self.y
end
function P:tag(k)
  if k % 7 == 0 then return self.name .. "!" end
  return self.name
end
local function run(n)
  local ps = { P.new(1, 2, "a"), P.new(3.5, -1, "b"), P.new(0, 0, "c") }
  local acc, fl, cnt, last, flag = 0, 0.0, 0, "", false
  local t = {}
  for i = 1, n do
    local p = ps[i % 3 + 1]
    local r = p:step(i, i * 0.25)
    fl = fl + r
    if r > 10 then cnt = cnt + 1; flag = not flag end
    last = p:tag(i)
    if #last > 1 then acc = acc + i else acc = acc - 1 end
    if i % 11 == 0 then t[#t + 1] = last end
  end
  return acc, string.format("%.6f", fl), cnt, last, flag, #t
end
for round = 1, 4 do
  print(round, run(400 + round * 37))
end
"#,
    },
    Prog {
        name: "s50_spill_values",
        small: false,
        src: r#"
-- many live values of every class across the side exits: more than the registers, so the parent's exit map has spill slots as sources
local function f(n)
  local a, b, c, d, e = 1, 2, 3, 4, 5
  local x1, x2, x3, x4 = 0.5, 1.5, 2.5, 3.5
  local s1, s2 = "p", "q"
  local t1, t2 = { 1 }, { 2 }
  local g, h, i2, j2 = 10, 20, 30, 40
  for i = 1, n do
    a = a + i; b = b + a % 7; c = c + b % 5; d = d ~ c; e = e + d % 3
    x1 = x1 + 0.25; x2 = x2 * 1.0001; x3 = x3 - 0.125; x4 = x4 + x1 * 0.001
    g = g + 1; h = h + g % 3; i2 = i2 + h % 2; j2 = j2 + i2 % 4
    if i % 4 == 0 then
      s1, s2 = s2, s1
      t1[1] = t1[1] + a % 3
    elseif i % 4 == 1 then
      t2[1] = t2[1] + b % 3
      x2 = x2 + 1
    elseif i % 9 == 2 then
      a = a - 3; x3 = x3 + c * 0.5
    end
    if i % 13 == 5 then t1, t2 = t2, t1 end
  end
  return a, b, c, d, e, string.format("%.5f %.5f %.5f %.5f", x1, x2, x3, x4), s1, s2, t1[1], t2[1], g, h, i2, j2
end
for r = 1, 3 do print(f(300 + 50 * r)) end
"#,
    },
    Prog {
        name: "s50_observers",
        small: false,
        src: r#"
-- the stack after a linked exit: errors raised in tier 1 behind a side trace (debug.getlocal of the frame), a count hook installed from inside, collections
local function body(n, fail_at)
  local sum, fsum, name, tab = 0, 0.0, "s", {}
  for i = 1, n do
    local v = i % 5
    if v == 0 then sum = sum + i
    elseif v == 1 then fsum = fsum + i * 0.5
    elseif v == 2 then name = (i % 2 == 0) and "even" or "odd"
    else tab[#tab + 1] = i end
    if i == fail_at then
      local locs = {}
      for k = 1, 6 do
        local nm, val = debug.getlocal(1, k)
        if nm and type(val) ~= "table" then locs[#locs + 1] = nm .. "=" .. tostring(val) end
      end
      error("stop " .. table.concat(locs, ","))
    end
  end
  return sum, fsum, name, #tab
end
for r = 1, 3 do
  print(pcall(body, 500, 300 + r * 40))
  print(body(400, -1))
end
local hits = 0
local function hooked(n)
  local s, f = 0, 0.0
  for i = 1, n do
    if i % 3 == 0 then s = s + i else f = f + i * 0.25 end
    if i == n // 2 then debug.sethook(function() hits = hits + 1 end, "", 97) end
  end
  debug.sethook()
  return s, f
end
print(hooked(800), hits > 0)
collectgarbage()
local keep = {}
local function churn(n)
  local acc = 0
  for i = 1, n do
    local t = { i, i * 2, tostring(i) }
    if i % 4 == 0 then keep[#keep + 1] = t; acc = acc + #t[3] else acc = acc + t[2] % 7 end
    if i % 50 == 0 then collectgarbage("step") end
  end
  return acc, #keep
end
for r = 1, 3 do print(churn(300)) end
"#,
    },
    Prog {
        name: "s50_coro_flush",
        small: false,
        src: r#"
-- the loop runs inside a coroutine that yields between batches (never on trace); between rounds a callee is replaced by a `load`ed function,
-- so the side traces that inlined it are flushed while their parents live (their links are undone) and new ones are attached
local co = coroutine.wrap(function(n)
  local acc, f = 0, 0.0
  while true do
    for i = 1, n do
      if i % 3 == 0 then acc = acc + i elseif i % 3 == 1 then f = f + i * 0.5 else acc = acc - 1 end
    end
    n = coroutine.yield(acc, string.format("%.3f", f))
  end
end)
for r = 1, 5 do print(co(200 + r)) end
local helper
local function make(k) return load("return function(x) if x % 2 == 0 then return x * " .. k .. " else return x + " .. k .. " end end")() end
local function loop(n)
  local s, t = 0, 0.0
  for i = 1, n do
    local v = helper(i)
    if v > 50 then s = s + v else t = t + v * 0.5 end
  end
  return s, t
end
for r = 1, 4 do
  helper = make(r)
  print(r, loop(300))
  collectgarbage()
end
"#,
    },
];
