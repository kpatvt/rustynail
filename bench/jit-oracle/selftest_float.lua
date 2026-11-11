-- Oracle self-test (not a benchmark): float loops with negative step, NaN comparisons, int/float division.
local function f(N)
  local c = 0.0
  local z = 0.0
  local nan = z / z
  local inf = 1.0 / z
  if nan == nan then c = c + 1.0 end
  if nan ~= nan then c = c + 2.0 end
  if nan < 1.0 then c = c + 4.0 end
  if nan <= 1.0 then c = c + 8.0 end
  if 1.0 < nan then c = c + 16.0 end
  if not (nan < 1.0) then c = c + 32.0 end
  if not (nan >= 1.0) then c = c + 64.0 end
  if inf > 1.0 then c = c + 128.0 end
  if -inf < 1.0 then c = c + 256.0 end
  if inf == inf then c = c + 512.0 end
  local s = 0.0
  for x = 10.0, 1.0, -0.5 do s = s + x end
  for x = 1.0, 10.0, 0.25 do s = s + x * 2.0 end
  for x = 3.0, 1.0, 1.0 do s = s + 1000.0 end
  local q = N / 4
  if q == 2.5 then c = c + 1024.0 end
  if s > 100.0 and s <= 1000.0 then c = c + 2048.0 end
  if N - 3 < 10 then c = c + 4096.0 end
  if N >= 10 then c = c + 8192.0 end
  return s + c
end
print(f(tonumber(arg and arg[1]) or 10))
