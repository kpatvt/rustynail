//! Section 76 programs (docs/JIT_TRACE.md section 76: compile cost). `s76_side_fail`: side exits whose recordings fail again and again (a C
//! function, a callee loop, a deep recursion, a closure, sorting with a Lua comparator), so that the back-off of failed side recordings
//! (`sideback`, 76.2) decides when, and whether, the next attempt is made. The results must not depend on it.

use super::progs::Prog;

pub const S76_PROGS: &[Prog] = &[
    Prog {
        name: "s76_hdr_reuse",
        small: false,
        src: r##"
-- repeated field reads of one table around header barriers (section 76.11, `hdrcse`): key insertions that rehash (integer keys in
-- the hash part) and allocations; a read after the barrier must see the new node vector and size
local acc = 0
for r = 1, 200 do
  local t = { a = 1, b = 2, c = 3 }
  for i = 1, 40 do
    acc = acc + t.a + t.b
    t[1000 + i * 7 + r] = i
    acc = acc + t.b + t.c
    local u = { x = t.a, y = i }
    acc = acc + u.x + t.a + u.y
    t[2000 + i] = u
    acc = acc + t.c + t.b
  end
end
print(acc)
"##,
    },
    Prog {
        name: "s76_t1_exits",
        small: false,
        src: r##"
-- tier-1 exits of every kind (section 76.10, `t1exit`): unsupported instructions (exit stubs), guards that leave with a refund
-- in the middle of a block, metamethod fallbacks, and allocation helpers whose collection runs a finalizer that installs a count
-- hook (the trap exit); the hook counts, so the exit's pc and refund must be exact
local log = {}
local hooked = 0
local first, cur = -1, 0
local function mk(i)
  return setmetatable({ i = i }, { __gc = function(o)
    if o.i % 49 == 0 and hooked == 0 then
      hooked = 1
      local n = 0
      debug.sethook(function()
        n = n + 1
        if n == 1 then first = cur end
        if n > 2000 then debug.sethook(); hooked = 2 end
      end, "", 7)
    end
  end })
end
local V = setmetatable({}, { __add = function(a, b) return 3 end })
-- an `__index` chain of three levels (the inline chain walk of tier 1 reads up to `idx_levels` of them)
local C0 = { top = 1 }
local C1 = setmetatable({ mid = 2 }, { __index = C0 })
local C2 = setmetatable({}, { __index = C1 })
local C3 = setmetatable({ deep = 3 }, { __index = C2 })
local acc = 0
for round = 1, 6 do
  for i = 1, 300 do
    cur = round * 1000 + i
    local t = { i, i + 1, x = i }
    if i % 7 == 0 then t = mk(i) end
    local v = (i % 11 == 0) and V or i
    acc = acc + (v + 1) + #t
    if i % 13 == 0 then acc = acc + select("#", i, round) end
    acc = acc + (C3.deep or 0) + (C2.mid or 0) + (C3.top or 0) + (C3.none or 5)
    local s = tostring(i)
    acc = acc + #s
  end
  collectgarbage()
  log[#log + 1] = acc
end
print(acc, hooked, first, table.concat(log, ","))
"##,
    },
    Prog {
        name: "s76_side_fail",
        small: false,
        src: r#"
-- side exits whose recordings fail again and again (a C function, a callee loop, leaving the frame, a deep recursion), so that the back-off
-- of failed side recordings decides when (and whether) the next attempt is made; results must not depend on it
local floor, fmt = math.floor, string.format
local function deep(n) if n == 0 then return 0 end return 1 + deep(n - 1) end
local function inner(n) local s = 0; for j = 1, n do s = s + j end; return s end
local acc, out = 0, {}
local tmp = {}
for o = 1, 30 do
  for i = 1, 400 do
    local m = i % 8
    if m == 1 then
      acc = acc + #fmt("%5.2f", i / 3)
    elseif m == 3 then
      acc = acc + inner(i % 13 + 3)
    elseif m == 5 then
      acc = acc + deep(i % 40 + 20)
    elseif m == 7 then
      tmp[#tmp + 1] = i
      if #tmp > 50 then table.sort(tmp, function(a, b) return a > b end); tmp = {} end
    else
      acc = acc + floor(i / 2)
    end
  end
  if o % 10 == 0 then collectgarbage() end
  out[#out + 1] = acc % 100000
end
print(acc, table.concat(out, ","))
"#,
    },
];
