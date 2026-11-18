-- more live numeric locals than registers: carried values live in spill slots, with a table, an integer and a float among them
local function f(t, n)
  local a1, a2, a3, a4, a5, a6, a7, a8 = 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0
  local b1, b2, b3, b4, b5, b6, b7, b8 = 0.5, 0.25, 0.125, 1.5, 2.5, 3.5, 4.5, 5.5
  local c1, c2, c3 = 10, 20, 30
  for i = 1, n do
    local row = t[i]
    local x = row.x
    for j = 1, 2 do
      a1 = a1 + x * b1; a2 = a2 + x * b2; a3 = a3 + x * b3; a4 = a4 + x * b4
      a5 = a5 + x * b5; a6 = a6 + x * b6; a7 = a7 + x * b7; a8 = a8 + x * b8
      b1 = b1 + a8 * 1e-3; b2 = b2 + a7 * 1e-3; b3 = b3 + a6 * 1e-3; b4 = b4 + a5 * 1e-3
      c1 = c1 + j; c2 = c2 + i; c3 = c3 + c1 % 3
    end
    b5 = b5 + a1 * 1e-4; b6 = b6 + a2 * 1e-4; b7 = b7 + a3 * 1e-4; b8 = b8 + a4 * 1e-4
  end
  return a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8, b1 + b2 + b3 + b4 + b5 + b6 + b7 + b8, c1, c2, c3
end
local t = {}
for i = 1, 30 do t[i] = { x = i * 0.1 } end
for k = 1, 60 do f(t, 30) end
for _, n in ipairs { 30, 7, 1, 0, 30 } do print(n, string.format("%.6f %.6f", f(t, n)), select(3, f(t, n))) end
