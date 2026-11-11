-- Mandelbrot: escape-time fractal, rows packed 8 pixels per byte (port from the public are-we-fast-yet
-- "Mandelbrot" description; the byte packing uses arithmetic instead of shifts so the same source runs on
-- Lua 5.1 style engines).  Output: checksum = sum of the packed bytes, and the number of pixels inside the set.
local N = tonumber(arg and arg[1]) or 3
local SIZE = 400

local function mandelbrot(size)
  local sum, inside = 0, 0
  for y = 0, size - 1 do
    local ci = 2.0 * y / size - 1.0
    local bitNum, byteAcc = 0, 0
    for x = 0, size - 1 do
      local zr, zi, zrzr, zizi = 0.0, 0.0, 0.0, 0.0
      local cr = 2.0 * x / size - 1.5
      local z, escape = 0, 0
      while escape == 0 and z < 50 do
        zr = zrzr - zizi + cr
        zi = 2.0 * zr * zi + ci
        zrzr = zr * zr
        zizi = zi * zi
        if zrzr + zizi > 4.0 then escape = 1 end
        z = z + 1
      end
      if escape == 0 then inside = inside + 1 end
      byteAcc = byteAcc * 2 + escape
      bitNum = bitNum + 1
      if bitNum == 8 then
        sum = sum + byteAcc
        byteAcc, bitNum = 0, 0
      elseif x == size - 1 then
        for s = 1, 8 - size % 8 do byteAcc = byteAcc * 2 end
        sum = sum + byteAcc
        byteAcc, bitNum = 0, 0
      end
    end
  end
  return sum, inside
end

local sum, inside
for i = 1, N do sum, inside = mandelbrot(SIZE) end
print("mandelbrot", sum, inside)
