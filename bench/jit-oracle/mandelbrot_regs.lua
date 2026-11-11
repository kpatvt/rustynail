-- JIT oracle program 3: mandelbrot escape iteration over an NxN grid, locals only (no tables).
local function f(N)
  local count = 0
  local d = 2.0 / N
  local ci = -1.0
  for y = 1, N do
    local cr = -1.5
    for x = 1, N do
      local zr, zi, i = 0.0, 0.0, 0
      while i < 50 and zr * zr + zi * zi < 4.0 do
        zr, zi = zr * zr - zi * zi + cr, 2.0 * zr * zi + ci
        i = i + 1
      end
      if i == 50 then count = count + 1 end
      cr = cr + d
    end
    ci = ci + d
  end
  return count
end
print(f(tonumber(arg and arg[1]) or 1500))
