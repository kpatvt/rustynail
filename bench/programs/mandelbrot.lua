local N = tonumber(arg and arg[1]) or 1500
local count = 0
for y = 0, N - 1 do
  local ci = 2.0 * y / N - 1.0
  for x = 0, N - 1 do
    local cr = 2.0 * x / N - 1.5
    local zr, zi, i = 0.0, 0.0, 0
    while i < 50 and zr * zr + zi * zi < 4.0 do zr, zi = zr * zr - zi * zi + cr, 2.0 * zr * zi + ci; i = i + 1 end
    if i == 50 then count = count + 1 end
  end
end
print(count)
