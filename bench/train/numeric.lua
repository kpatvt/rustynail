-- PGO training (scripts/pgo.sh): numeric kernels unlike bench/programs (LU solve, game of life,
-- Collatz, trial-division primes, integer bit tricks).
local function lu_solve(n)
  local a, b = {}, {}
  for i = 1, n do
    a[i] = {}
    for j = 1, n do a[i][j] = 1.0 / (i + j - 1) + (i == j and n or 0) end
    b[i] = i * 0.5
  end
  for k = 1, n - 1 do
    for i = k + 1, n do
      local f = a[i][k] / a[k][k]
      local ai, ak = a[i], a[k]
      for j = k, n do ai[j] = ai[j] - f * ak[j] end
      b[i] = b[i] - f * b[k]
    end
  end
  local x = {}
  for i = n, 1, -1 do
    local s = b[i]
    for j = i + 1, n do s = s - a[i][j] * x[j] end
    x[i] = s / a[i][i]
  end
  local sum = 0.0
  for i = 1, n do sum = sum + x[i] end
  return sum
end

local function life(w, h, gens)
  local g = {}
  for y = 0, h - 1 do for x = 0, w - 1 do g[y * w + x] = ((x * 7 + y * 13) % 5 == 0) and 1 or 0 end end
  for _ = 1, gens do
    local n = {}
    for y = 0, h - 1 do
      for x = 0, w - 1 do
        local c = 0
        for dy = -1, 1 do
          for dx = -1, 1 do
            if dx ~= 0 or dy ~= 0 then
              c = c + g[((y + dy) % h) * w + (x + dx) % w]
            end
          end
        end
        local v = g[y * w + x]
        n[y * w + x] = (c == 3 or (v == 1 and c == 2)) and 1 or 0
      end
    end
    g = n
  end
  local alive = 0
  for i = 0, w * h - 1 do alive = alive + g[i] end
  return alive
end

local function collatz(limit)
  local best, besti = 0, 0
  for i = 1, limit do
    local n, steps = i, 0
    while n ~= 1 do
      if n & 1 == 0 then n = n >> 1 else n = 3 * n + 1 end
      steps = steps + 1
    end
    if steps > best then best, besti = steps, i end
  end
  return besti, best
end

local function primes(limit)
  local count = 0
  for n = 2, limit do
    local p = true
    local d = 2
    while d * d <= n do
      if n % d == 0 then p = false; break end
      d = d + 1
    end
    if p then count = count + 1 end
  end
  return count
end

local function floats(n)
  local s, t = 0.0, 1.0
  for i = 1, n do
    t = t * 1.0000001
    s = s + math.sin(i * 0.001) * t - math.sqrt(i) / (i + 1.5)
    if s > 1e6 then s = s / 3 end
  end
  return math.floor(s * 1000) // 1
end

print(string.format("%.6f", lu_solve(120)))
print(life(64, 48, 40))
print(collatz(150000))
print(primes(60000))
print(floats(400000))
