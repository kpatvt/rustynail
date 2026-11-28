//! Section 87 programs (docs/JIT_TRACE.md section 87: the fused back end, `asmfuse`). `s87_fuse_arith`: side traces made of stack loads,
//! integer / float arithmetic and compare guards with many values live at once (spills, reloads, exit maps shared by several guards).

use super::progs::Prog;

pub const S87_PROGS: &[Prog] = &[
    Prog {
        name: "s87_fuse_arith",
        small: false,
        src: r#"
-- side traces made of stack loads, integer / float arithmetic and compare guards (section 87, `asmfuse`): many values live at once in
-- the side branches (spills and reloads), guards sharing a snapshot (pinned exit maps), constants in compares
local s, t, u = 0, 0.5, 7
local a, b, c, d, e, f, g, h = 1, 2, 3, 4, 5, 6, 7, 8
local k1, k2, k3, k4 = 9, 10, 11, 12
local p, q, r, w = 1.25, 2.5, 3.75, 0.125
for o = 1, 6 do
  for i = 1, 300 do
    local m = i & 7
    if m == 0 then
      s = s + i * 2
    elseif m == 3 and o > 3 then
      a, b, c, d = a + i, b ~ i, c - a, d + b
      e, f, g, h = e + c, f ~ d, g + e - f, h + a * 3
      k1, k2, k3, k4 = k1 + e - h, k2 ~ (a + g), k3 + (b & 255), k4 - (c | d)
      s = s + (a ~ b) - (c & d) + (e | f) - g + h + (k1 ~ k2) - (k3 & k4)
      a, b, c, d = a & 0xFFFFF, b & 0xFFFFF, c & 0xFFFFF, d & 0xFFFFF
      e, f, g, h = e & 0xFFFFF, f & 0xFFFFF, g & 0xFFFFF, h & 0xFFFFF
      k1, k2, k3, k4 = k1 & 0xFFFFF, k2 & 0xFFFFF, k3 & 0xFFFFF, k4 & 0xFFFFF
    elseif m == 5 then
      p, q, r, w = p * 0.5 + q * 0.25, q - p * 0.25, r / 2 + w * 0.5, w * 0.5 + p - r * 0.25
      t = t + p * q - r * w
      if t > 1e6 then t = t / 1024 end
    else
      s = s ~ i
    end
    if t < 100.0 then t = t * 1.5 + i else t = t - 99.5 end
    if s > 100000 then s = s - 99991 elseif s < -100000 then s = s + 99989 end
    u = (u * 33 + i) & 0xFFFF
  end
end
print(s, string.format("%.6g %.6g %.6g %.6g %.6g", t, p, q, r, w), u, a, b, c, d, e, f, g, h, k1, k2, k3, k4)
"#,
    },
    Prog {
        name: "s87_fuse_fields",
        small: false,
        src: r#"
-- side traces over table fields (section 87): HREFK / HLOAD pairs, shape guards, field stores, array loads and upvalues, with several guards
-- of one bytecode sharing a snapshot and more values live than registers under `asmfuse=2`
local pts = {}
for i = 1, 64 do pts[i] = {x = i * 0.5, y = i % 7, z = i, w = -i, v = i * 3, name = "p" .. i} end
local acc, cnt, up = 0, 0, 3
local function bump(k) up = up + k end
for o = 1, 40 do
  for i = 1, 64 do
    local p = pts[i]
    if p.y > 3 and o > 2 then
      acc = acc + p.x * p.y - p.z + p.w * 2 + p.v + up
      if p.x + p.z > p.v - p.w and p.y < p.z then cnt = cnt + 1 end
      p.z = p.z + p.y; p.w = p.w - 1
      local q = pts[(i % 64) + 1]
      if q.z > p.z then acc = acc + (q.z - p.z) * q.y else acc = acc - q.w end
    elseif p.y == 2 and o > 4 then
      bump(p.y); p.v = p.v + up % 5
    else
      acc = acc - p.y
    end
  end
  if o % 8 == 0 then for i = 1, 64, 5 do pts[i]["k" .. o] = o end end
end
print(acc, cnt, up, pts[7].z, pts[9].w, pts[13].v)
"#,
    },
];
