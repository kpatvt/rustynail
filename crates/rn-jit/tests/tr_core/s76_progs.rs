//! Section 76 programs (docs/JIT_TRACE.md section 76: compile cost). `s76_side_fail`: side exits whose recordings fail again and again (a C
//! function, a callee loop, a deep recursion, a closure, sorting with a Lua comparator), so that the back-off of failed side recordings
//! (`sideback`, 76.2) decides when, and whether, the next attempt is made. The results must not depend on it.

use super::progs::Prog;

pub const S76_PROGS: &[Prog] = &[Prog {
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
}];
