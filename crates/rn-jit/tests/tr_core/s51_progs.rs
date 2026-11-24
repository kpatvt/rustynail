//! Section 51 programs (`rapol`: policy levers of the trace register allocator, docs/JIT_TRACE.md section 51). Loop bodies with more live values
//! than registers (integers and floats, loop-carried values rotated across the back-edge, invariants read in the body), values live across helper
//! calls that are always made (table constructors, concatenation, number-to-string, `string.rep`) in loops and in straight-line traces, and exits
//! (type flips, branch flips) at every point, so that snapshots restore values from registers, from spill slots and from `rbp`.

use super::progs::Prog;

pub const S51_PROGS: &[Prog] = &[
    Prog {
        name: "s51_loop_pressure",
        small: false,
        src: r#"
-- a loop body with about twelve live integers and floats: evictions inside the body, PHIs in registers and in spill slots, a rotation of
-- loop-carried values (a parallel move with cycles at the back-edge) and invariants read in the body
local function run(n, k0, s0)
  local a, b, c, d, e = 1, 2, 3, 4, 5
  local x, y, z = 0.5, 1.25, -2.0
  local p, q = k0, k0 * 2
  local acc, facc = 0, 0.0
  local inv1, inv2, inv3, inv4 = k0 + 1, k0 + 2, s0 * 0.5, s0 - 1
  for i = 1, n do
    local t = a + b * 3 - c
    local u = d ~ e
    local w = (t + u + i) % 1000
    a, b, c, d, e = b, c, d, e, (w + inv1) % 97
    x, y, z = y + 0.5, z * 0.5 + inv3, x - y
    p, q = q, p + inv2
    acc = acc + t + u + w + p - q + inv4
    facc = facc + x * y - z
    if i % 97 == 0 then acc = acc + 0.5 end -- the accumulator becomes a float: an exit from inside the body
    if i % 131 == 0 then facc = facc - acc % 7 end
  end
  return a, b, c, d, e, string.format("%.6f %.6f %.6f", x, y, z), p, q, acc, string.format("%.6f", facc)
end
for round = 1, 4 do
  print(round, run(300 + 41 * round, round, round * 1.5))
end
"#,
    },
    Prog {
        name: "s51_call_values",
        small: false,
        src: r#"
-- values live across helper calls that are always made, several calls in a row (the same value crosses all of them), in a loop and in the
-- straight-line side traces of its exits; floats and integers; the results are read after the calls
local rep, fmt = string.rep, string.format
local function run(n, salt)
  local keep = {}
  local i1, i2, i3, i4, i5 = salt, salt * 3, salt + 7, salt - 2, 11
  local f1, f2, f3 = salt * 0.5, 1.75, -3.5
  local s = "s" .. salt
  local acc = 0
  for i = 1, n do
    local t = { i, i1, f1 }        -- TNEW (+ stores)
    local str = s .. i              -- CONCAT with a number
    local str2 = tostring(i2 + i)   -- number to string
    local r = rep("ab", (i % 3) + 1)
    acc = acc + #str + #str2 + #r + t[1] + i1 + i2 + i3 + i4 + i5
    f1, f2, f3 = f2 + 0.25, f3 * 0.5, f1 - f2
    i1, i2, i3 = i2 % 1000, i3 + 1, i1 + i4
    if i % 17 == 0 then keep[#keep + 1] = t end
    if i % 29 == 0 then i5 = i5 + 0.5 end -- type flip of a value live across the calls
    if i % 23 == 0 then s = s .. "x"; if #s > 40 then s = "s" end end
  end
  return acc, fmt("%.6f %.6f %.6f", f1, f2, f3), i1, i2, i3, i4, i5, #keep
end
for round = 1, 4 do
  print(round, run(250 + 31 * round, round))
end
"#,
    },
    Prog {
        name: "s51_phi_rotate",
        small: false,
        src: r#"
-- rotations of loop-carried values of both classes (swaps and three-cycles at the back-edge), with a helper call in the body and an
-- early exit whose snapshot holds every rotated value
local function run(n)
  local a, b, c = 1, 2, 3
  local x, y = 0.25, 4.0
  local m, k = 7, 9
  local out = 0
  for i = 1, n do
    a, b, c = c, a, b + i
    x, y = y, x + 0.5
    m, k = k, m
    if i % 5 == 0 then
      local t = { a, b, c }
      out = out + #t + t[3]
    end
    if c > 4000000 then break end
  end
  return a, b, c, string.format("%.4f %.4f", x, y), m, k, out
end
for round = 1, 3 do
  print(round, run(500 + round * 77))
end
"#,
    },
];
