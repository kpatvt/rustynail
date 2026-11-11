-- J5 experiment X3 `short_inner` (docs/JIT_TRACE.md 1.4, 9.9): a short inner `while` loop (mandelbrot-like escape
-- loop, at most 20 iterations) executed many times, so a trace pays an entry and a loop-exit exit per execution.
-- N = reps (arg[1]) of the row loop; 64 pixels per row.
-- Expected stdout (default N): see short_inner.expected
local function pixel(cr, ci)
  local zr, zi, k = 0.0, 0.0, 0
  while k < 20 and zr * zr + zi * zi < 4.0 do
    zr, zi = zr * zr - zi * zi + cr, 2.0 * zr * zi + ci
    k = k + 1
  end
  return k
end
local function row(ci, w)
  local total = 0
  for x = 0, w - 1 do total = total + pixel(-2.0 + x * (3.0 / w), ci) end
  return total
end
local reps = tonumber(arg and arg[1]) or 180000
local total = 0
for r = 1, reps do total = total + row(((r % 64) - 32) / 32.0, 64) end
print(total)
