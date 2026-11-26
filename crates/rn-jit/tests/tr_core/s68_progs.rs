//! Section 68 programs (`regcall`: `LIB` calls pass their operands in the C argument registers to register entries of the helpers, the result
//! comes back in `rax`, docs/JIT_TRACE.md section 68). Each loop keeps several values live across the calls (saved / reloaded around them) and
//! feeds the calls operands from changing registers and constants; the helpers' "did nothing" answers (a type change, a pattern with specials,
//! a collection due, a result kind that differs from the recorded one) exit at the guard behind the call.

use super::progs::Prog;

pub const S68_PROGS: &[Prog] = &[
    Prog {
        name: "s68_find_calls",
        small: false,
        src: r#"
-- plain, no-specials, pattern and match finds with live values around them; start positions positive, negative, beyond the end
local S, W = {}, { "quick", "brown", "fox", "o", "dog", "zz" }
for k = 1, 48 do S[k] = "the quick brown fox jumps over " .. k .. " dog" .. string.rep("-", k % 7) end
local acc, a2, a3, a4 = 0, 0, 0, 0
for o = 1, 30 do
  for k = 1, #S do
    local s, w = S[k], W[k % #W + 1]
    local i = (k + o) % 9 - 4
    acc = acc + (string.find(s, w, 1, true) or -1)
    a2 = a2 + (string.find(s, w, i, true) or -2) + (string.find(s, w) or 0)
    a3 = a3 + (string.find(s, "%d+", i) or -3) + (string.find(s, "o.", k % 5 + 1) or 0)
    local m = string.match(s, "(%a+) dog", 1)
    a4 = a4 + (m and #m or 0) + (string.match(s, "%d+") and 1 or 0)
  end
end
print(acc, a2, a3, a4)
-- a pattern operand that turns into one with specials, then a number subject
local P = { "fox", "fox", "f.x", "fox" }
local c = 0
for k = 1, 200 do c = c + (string.find("a fox and a fax", P[k % 4 + 1], 1) or 0) end
print(c)
for k = 1, 120 do c = c + (string.find(k < 100 and "x12y" or 1234, "2", 1, true) or 0) end
print(c)
"#,
    },
    Prog {
        name: "s68_sub_char_calls",
        small: false,
        src: r#"
-- sub / char / rep / lower / upper / reverse with values live across the calls; allocations (a collection may be due: the helper declines)
local t, s = {}, "abcdefghijklmnopqrstuvwxyz0123456789"
local long = string.rep("Long-String-", 5)
local n, h, x, y, z = 0, 0, 3, 5, 7
for k = 1, 3000 do
  local i, j = k % 11 - 3, k % 17 - 5
  local a = string.sub(s, i, j)
  local b = string.sub(long, x, y + k % 40)
  local c = string.char(65 + k % 26, 97 + (k + x) % 26)
  local d = string.rep(c, k % 4, ",")
  local e = string.lower(b) .. string.upper(a) .. string.reverse(c)
  n = n + #a + #b + #c + #d + #e
  h = (h * 31 + string.byte(e, 1 + k % #e)) % 1000003
  x, y, z = y, z, x
  if k % 500 == 0 then t[#t + 1] = e end
end
print(n, h, x, y, z, #t, t[1], t[#t])
-- a subject that stops being a string
local V = {}
for k = 1, 100 do V[k] = k < 90 and ("v" .. k) or k end
local m = 0
for k = 1, 100 do m = m + #string.sub(V[k], 2, -1) end
print(m)
"#,
    },
    Prog {
        name: "s68_cmp_calls",
        small: false,
        src: r#"
-- long-string compares (the `LIB streq` / `strlt` / `strle` helpers) with many live values and permuted operands
local base = string.rep("x", 60)
local L = {}
for k = 1, 64 do L[k] = base .. string.char(65 + k % 23) .. k end
local eq, lt, le, q1, q2, q3 = 0, 0, 0, 1, 2, 3
for o = 1, 40 do
  for k = 1, #L do
    local a, b = L[k], L[(k * 7 + o) % #L + 1]
    if a == b then eq = eq + 1 end
    if a < b then lt = lt + q1 end
    if b <= a then le = le + q2 end
    if (L[(k + 1) % #L + 1] < a) == (q3 > 2) then q1, q2, q3 = q2, q3, q1 end
  end
end
print(eq, lt, le, q1, q2, q3)
"#,
    },
    Prog {
        name: "s68_mixed_calls",
        small: false,
        src: r#"
-- format / tostring / random / gmatch / type next to the string calls; an error raised by a library call on a later iteration
math.randomseed(7)
local acc, f, cnt = 0, 0.5, 0
local words = "alpha beta gamma delta epsilon zeta eta theta"
for k = 1, 2500 do
  local s = string.format("%d:%s", k, tostring(k * 3))
  acc = acc + #s + math.random(1, 9) + (type(s) == "string" and 1 or 0)
  f = f + math.random()
  if k % 250 == 0 then for w in string.gmatch(words, "%a+") do cnt = cnt + #w end end
end
print(acc, f > 0, cnt)
local ok, err = pcall(function()
  local r = 0
  for k = 1, 300 do r = r + #string.rep("ab", k % 5, k < 290 and "," or {}) end
  return r
end)
print(ok, (tostring(err):gsub("^.-:%d+: ", "")))
"#,
    },
];
