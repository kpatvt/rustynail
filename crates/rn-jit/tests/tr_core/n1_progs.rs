//! N1 programs (docs/JIT_TRACE.md section 41): the trace loop-body diet. Constant index offsets folded into the `ALOAD` / `ASTORE` displacement
//! (`ixfuse`), the negated element index carried as a second induction variable (`nidx`), the counted-loop back-edge that updates the index and
//! the count and tests the count in one branch (`cloop`). Each program prints values that expose a wrong displacement (another element's
//! value), an off-by-one count (one iteration too many or too few after an exit), a stale index after an exit and an overflowing index.

use super::progs::Prog;

pub const N1_PROGS: &[Prog] = &[
    Prog {
        name: "n1_offsets",
        small: false,
        src: r#"
-- constant offsets of both signs in loads and stores of one loop, int and float elements, edges of the array
local function f(t, n)
  local s = 0
  for i = 4, n - 3 do
    s = s + t[i - 3] + t[i + 3] * 2 + t[i - 1] * 3 + t[i + 1] * 5 + t[i]
    t[i + 2] = t[i - 2] + 1
  end
  return s
end
local function mk(n, k)
  local t = {}
  for i = 1, n do t[i] = i * k end
  return t
end
local out = {}
for r = 1, 40 do f(mk(30, 1), 30) end
for r = 1, 3 do
  local t = mk(30, r)
  out[#out + 1] = f(t, 30)
  out[#out + 1] = table.concat(t, ",")
end
-- floats
for r = 1, 3 do
  local t = mk(30, 0.5 * r)
  out[#out + 1] = f(t, 30)
  out[#out + 1] = table.concat(t, ",")
end
-- the loop reads one element past the array: nil arithmetic (error), at the first, a middle and the last iteration
for _, n in ipairs { 31, 32, 33, 34, 36 } do
  local t = mk(30, 1)
  out[#out + 1] = tostring(pcall(f, t, n)) .. n
  out[#out + 1] = table.concat(t, ",")
end
-- holes at the offsets
for hole = 1, 30, 4 do
  local t = mk(30, 1)
  t[hole] = nil
  local ok, e = pcall(f, t, 30)
  out[#out + 1] = tostring(ok) .. (ok and e or "err")
end
-- a type change at an offset element (a string, a float among integers)
local t = mk(30, 1)
t[20] = 2.5
out[#out + 1] = f(t, 30)
t[20] = "7"
out[#out + 1] = f(t, 30)
t[20] = {}
out[#out + 1] = tostring((pcall(f, t, 30)))
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "n1_steps",
        small: false,
        src: r#"
-- positive, negative and large steps with offsets, loops that run once or not at all, a count that is not a multiple of the step
local function up(t, a, b, st)
  local s = 0
  for i = a, b, st do s = s + (t[i + 1] or 0) * 3 + (t[i - 1] or 0) end
  return s
end
local function dn(t, a, b)
  local s = 0
  for i = a, b, -2 do s = s + t[i - 1] + t[i + 1] * 2 end
  return s
end
local function dn1(t, a, b)
  local s = 0
  for i = a, b, -1 do s = s + t[i + 2] - t[i - 2] end
  return s
end
local function big(t, a, b)
  local s = 0
  for i = a, b, 1000 do s = s + t[i + 1] end
  return s
end
local function st3(t, a, b)
  local s = 0
  for i = a, b, 3 do s = s + t[i] + t[i + 3] end
  return s
end
local t = {}
for i = 1, 60 do t[i] = i end
local out = {}
for r = 1, 40 do
  up(t, 2, 58, 1); dn(t, 59, 2); dn1(t, 58, 3); st3(t, 1, 54); big(t, 1, 50)
end
for _, a in ipairs { 1, 2, 5, 30 } do
  for _, b in ipairs { 1, 2, 3, 29, 30, 59, 60 } do
    out[#out + 1] = up(t, a, b, 1)
    out[#out + 1] = up(t, a, b, 7)
    out[#out + 1] = tostring((pcall(dn, t, b, a)))
    out[#out + 1] = tostring((pcall(dn1, t, b, a)))
    out[#out + 1] = tostring((pcall(st3, t, a, b)))
  end
end
out[#out + 1] = big(t, 1, 1)
out[#out + 1] = big(t, 59, 59)
out[#out + 1] = tostring((pcall(big, t, 1, 2000)))
out[#out + 1] = dn(t, 59, 2)
out[#out + 1] = dn1(t, 58, 3)
out[#out + 1] = st3(t, 1, 54)
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "n1_limits",
        small: false,
        src: r#"
-- loop counts and indices at the limits of the integers: the index or the limit wraps in a naive implementation
local function around(t, a, b)
  local s, n = 0, 0
  for i = a, b do
    n = n + 1
    local v = t[i + 1]
    if v then s = s + v end
    if n > 40 then break end
  end
  return s, n
end
local function down(t, a, b)
  local s, n = 0, 0
  for i = a, b, -1 do
    n = n + 1
    local v = t[i - 1]
    if v then s = s + v end
    if n > 40 then break end
  end
  return s, n
end
local function stepbig(a, b, st)
  local n = 0
  for i = a, b, st do n = n + 1 end
  return n
end
local t = {}
for i = 1, 50 do t[i] = i end
local out = {}
local mx, mn = math.maxinteger, math.mininteger
for r = 1, 40 do around(t, 1, 40); down(t, 40, 1); stepbig(1, 50, 1) end
local function show(...) out[#out + 1] = table.concat({ ... }, ":") end
show(around(t, mx - 3, mx))
show(around(t, mx - 41, mx))
show(around(t, mn, mn + 3))
show(around(t, -3, 3))
show(around(t, -1, 45))
show(around(t, 0, 0))
show(down(t, mn + 3, mn))
show(down(t, mx, mx - 3))
show(down(t, 3, -3))
show(down(t, 45, -1))
show(stepbig(mx - 5, mx, 1), stepbig(mx - 5, mx, 2), stepbig(mx - 5, mx, 5), stepbig(mx - 5, mx, 6), stepbig(mx - 5, mx, mx))
show(stepbig(mn, mn + 5, 1), stepbig(mn + 5, mn, -1), stepbig(mn + 5, mn, -5), stepbig(mn + 5, mn, -6), stepbig(1, 0, 1), stepbig(0, 1, -1))
show(stepbig(mn, mx, mx), stepbig(mx, mn, mn + 1), stepbig(-1, 1, 1))
-- a float limit and a string limit (the integer loop clips the limit)
local function fl(t, b)
  local s = 0
  for i = 1, b do s = s + (t[i - 1] or 0) + (t[i + 1] or 0) end
  return s
end
for r = 1, 40 do fl(t, 40.9) end
show(fl(t, 40.9), fl(t, 3.5), fl(t, 0.5), fl(t, -1e100), fl(t, "12"))
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "n1_exits",
        small: false,
        src: r#"
-- exits in the middle of an iteration and after the last one: the interpreter must continue with the right index and count (a loop that
-- runs a different number of iterations after the exit gives another sum), nested loops, early break, an index used after the loop
local function mid(t, n, flip)
  local s = 0
  for i = 2, n do
    local a = t[i - 1]
    if flip and i == flip then a = a .. "x" end
    s = s + #tostring(a) + t[i + 1 - 1]
  end
  return s
end
local function nest(m, n)
  local s = 0
  for i = 1, n do
    local row = m[i]
    for j = 1, n do s = s + row[j] * (i - j) + m[j][i] end
  end
  return s
end
local function brk(t, n, at)
  local s, last = 0, 0
  for i = 1, n do
    s = s + t[i + 1]
    last = i
    if i == at then break end
  end
  return s, last
end
local t = {}
for i = 1, 40 do t[i] = i end
local m = {}
for i = 1, 12 do m[i] = {} for j = 1, 12 do m[i][j] = i * 100 + j end end
local out = {}
for r = 1, 40 do mid(t, 39); nest(m, 12); brk(t, 39, 0) end
for _, flip in ipairs { 2, 3, 20, 39, false } do out[#out + 1] = mid(t, 39, flip) end
for _, at in ipairs { 0, 1, 2, 19, 38, 39 } do out[#out + 1] = table.concat({ brk(t, 39, at) }, ":") end
out[#out + 1] = nest(m, 12)
m[7][7] = "z"
out[#out + 1] = tostring((pcall(nest, m, 12)))
m[7][7] = 1.5
out[#out + 1] = nest(m, 12)
m[7] = nil
out[#out + 1] = tostring((pcall(nest, m, 12)))
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "n1_matrix",
        small: false,
        src: r#"
-- the inner loops of a matrix product and a vector update: two induction-indexed accesses in one body (shared negated index), floats and ints
local function mul(a, b, c, n)
  for i = 1, n do
    local ai, ci = a[i], c[i]
    for j = 1, n do
      local s = 0.0
      for k = 1, n do s = s + ai[k] * b[k][j] end
      ci[j] = s
    end
  end
end
local function mk(n, f)
  local m = {}
  for i = 1, n do
    m[i] = {}
    for j = 1, n do m[i][j] = f(i, j) end
  end
  return m
end
local n = 9
local a = mk(n, function(i, j) return (i + j) % 5 + 0.5 end)
local b = mk(n, function(i, j) return (i * j) % 7 - 2 end)
local c = mk(n, function() return 0 end)
for r = 1, 30 do mul(a, b, c, n) end
local out = {}
for i = 1, n do out[#out + 1] = table.concat(c[i], ",") end
-- a shorter matrix than n: the access past the end
local ok = pcall(mul, a, b, c, n + 1)
out[#out + 1] = tostring(ok)
b[5][3] = nil
out[#out + 1] = tostring((pcall(mul, a, b, c, n)))
b[5][3] = 3
b[4] = {}
out[#out + 1] = tostring((pcall(mul, a, b, c, n)))
print(table.concat(out, "\n"))
"#,
    },
    Prog {
        name: "n1_fill",
        small: false,
        src: r#"
-- stores through an offset index: fills, shifts (overlapping reads and writes), and a grow-by-one loop
local function shift(t, n)
  for i = 1, n - 1 do t[i] = t[i + 1] end
  t[n] = 0
end
local function shiftr(t, n)
  for i = n, 2, -1 do t[i] = t[i - 1] end
  t[1] = 0
end
local function fill(t, n)
  for i = 0, n - 1 do t[i + 1] = i * 2 end
end
local function grow(t, n)
  for i = 1, n do t[#t + 1] = i end
end
local function mixed(t, n)
  for i = 1, n do t[i + 2] = i % 2 == 0 and i or i + 0.5 end
end
local t = {}
for i = 1, 30 do t[i] = i end
for r = 1, 40 do shift(t, 30); shiftr(t, 30); fill(t, 30); mixed(t, 28) end
local out = {}
fill(t, 30); shift(t, 30); out[#out + 1] = table.concat(t, ",")
fill(t, 30); shiftr(t, 30); out[#out + 1] = table.concat(t, ",")
mixed(t, 28); out[#out + 1] = table.concat(t, ",")
local g = {}
for r = 1, 40 do g = {} grow(g, 20) end
out[#out + 1] = table.concat(g, ",")
fill(g, 25); out[#out + 1] = table.concat(g, ",")
mixed(g, 30); out[#out + 1] = table.concat(g, ",")
print(table.concat(out, "\n"))
"#,
    },
];
