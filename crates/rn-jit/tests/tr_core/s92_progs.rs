//! Section 92 programs (docs/JIT_TRACE.md section 92): `sretry` (the third side attempt at an exit whose second ended `SideShort` is not held
//! to `sidemin`), `libsite` (the outcome guards of a `LIB` helper are ordinary exit sites with side traces) and `rerootgain` (the `reroot` rule
//! fires only when the side trace's path promises more completed iterations). Each program has short paths from a branch or a loop end back
//! to a traced loop head, string-library outcomes that change between phases, observers on those paths (errors caught by `pcall`, collections,
//! a count hook) and a type change at the end.

use super::progs::Prog;

pub const S92_PROGS: &[Prog] = &[
    Prog {
        name: "s92_short_tails",
        small: false,
        src: r#"
-- the diff_levenshtein inner loop: three data-dependent branches whose tails are one to three bytecodes before the back-edge, stores that grow
-- the rows (a new key on the first long word), loop ends every few iterations
local seed = 2024
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end
local syll = { "ka", "to", "mi", "re", "su", "no", "la", "vi" }
local function word()
  local p = {}
  for i = 1, 1 + rnd(4) do p[i] = syll[rnd(#syll)] end
  return table.concat(p)
end
local function lev(a, b, prev, cur)
  local la, lb = #a, #b
  for j = 0, lb do prev[j] = j end
  for i = 1, la do
    cur[0] = i
    local ca = a:byte(i)
    for j = 1, lb do
      local cost = (ca == b:byte(j)) and 0 or 1
      local best = prev[j] + 1
      if cur[j - 1] + 1 < best then best = cur[j - 1] + 1 end
      if prev[j - 1] + cost < best then best = prev[j - 1] + cost end
      cur[j] = best
    end
    prev, cur = cur, prev
  end
  return prev[lb]
end
local words = {}
for i = 1, 60 do words[i] = word() end
local prev, cur = {}, {}
local total, hist = 0, {}
for r = 1, 3 do
  for i = 1, #words do
    for j = i + 1, math.min(#words, i + 12) do
      local d = lev(words[i], words[j], prev, cur)
      total = total + d
      hist[d] = (hist[d] or 0) + 1
    end
    if i % 17 == 0 then collectgarbage("step") end
  end
end
-- a sieve: the counting branch is a two-bytecode tail
local n, flags, count = 3000, {}, 0
for i = 2, n do flags[i] = true end
for i = 2, n do
  if flags[i] then
    count = count + 1
    for k = i + i, n, i do flags[k] = false end
  end
end
-- an error raised on a short tail, caught outside the loop; a count hook installed from inside
local function tail_err(t)
  local s = 0
  for i = 1, #t do
    if t[i] > 7 then s = s + 1 end
    if i == 900 then error("tail " .. s) end
  end
  return s
end
local arr = {}
for i = 1, 1200 do arr[i] = rnd(10) end
local ok, msg = pcall(tail_err, arr)
local hooked = 0
local function hooky(t)
  local s = 0
  for i = 1, #t do
    if t[i] < 3 then s = s + 1 end
    if i == 400 then debug.sethook(function() hooked = hooked + 1 end, "", 50) end
  end
  debug.sethook()
  return s
end
local hs = hooky(arr)
-- the tails turn float at the end
local f = 0
for i = 1, 2000 do
  if arr[(i % #arr) + 1] > 5 then f = f + 1 end
  if i == 1800 then f = f + 0.5 end
end
local hk = {}
for k, v in pairs(hist) do hk[#hk + 1] = k .. ":" .. v end
table.sort(hk)
print(total, count, ok, msg, hs, hooked > 0, f, table.concat(hk, " "))
"#,
    },
    Prog {
        name: "s92_lib_outcomes",
        small: false,
        src: r#"
-- a tokenizer: `c:match` / `string.find` outcomes decide the branch; which outcome the root records depends on the iteration that made the
-- loop hot, and the majority changes between phases
local function tokenize(src)
  local out, i = {}, 1
  while i <= #src do
    local c = src:sub(i, i)
    if c:match("%s") then
      i = i + 1
    elseif c:match("%d") then
      local num = src:match("^%d+", i)
      out[#out + 1] = tonumber(num)
      i = i + #num
    elseif c:match("[%a_]") then
      local id = src:match("^[%a_][%w_]*", i)
      out[#out + 1] = id
      i = i + #id
    else
      out[#out + 1] = c
      i = i + 1
    end
  end
  return out
end
local function count_plain(s, pat)
  local n, i = 0, 1
  while true do
    local a, b = string.find(s, pat, i, true)
    if not a then break end
    n = n + 1
    i = b + 1
  end
  return n
end
local parts = {}
for i = 1, 300 do
  parts[#parts + 1] = (i % 3 == 0) and ("x" .. i) or tostring(i * 7)
  parts[#parts + 1] = (i % 5 == 0) and " + " or " "
end
local src1 = table.concat(parts)
local src2 = string.rep("ab_c1 ", 200) .. string.rep("    ", 300) .. string.rep("(1)", 100)
local sum, ids, other = 0, 0, 0
for r = 1, 4 do
  local src = (r % 2 == 1) and src1 or src2
  local t = tokenize(src)
  for _, v in ipairs(t) do
    if type(v) == "number" then sum = sum + v elseif #v > 1 then ids = ids + 1 else other = other + 1 end
  end
  if r == 2 then collectgarbage() end
end
local found = count_plain(src1, "x1") + count_plain(src2, "ab") + count_plain(src2, "zz")
-- an error behind a find outcome
local ok, msg = pcall(function()
  local n = 0
  for i = 1, 500 do
    if string.find(src1, tostring(i), 1, true) then n = n + 1 end
    if i == 450 then error("find " .. n) end
  end
end)
print(sum, ids, other, found, ok, msg)
"#,
    },
];
