local floor = math.floor
local n = tonumber(arg and arg[1]) or 1200000
local a = {}
local seed = 42
-- exact (seed * 1103515245 + 12345) % 2^31 without exceeding 2^53 (LuaJIT numbers are doubles)
local AH, AL = floor(1103515245 / 65536), 1103515245 % 65536
for i = 1, n do
  seed = (seed * AL + ((seed * AH) % 32768) * 65536 + 12345) % 2147483648
  a[i] = seed
end
local function heapsort(a, n)
  local function sift(lo, hi)
    local root = lo
    while true do
      local child = root * 2
      if child > hi then break end
      if child < hi and a[child] < a[child + 1] then child = child + 1 end
      if a[root] < a[child] then a[root], a[child] = a[child], a[root]; root = child else break end
    end
  end
  for i = floor(n / 2), 1, -1 do sift(i, n) end
  for i = n, 2, -1 do a[1], a[i] = a[i], a[1]; sift(1, i - 1) end
end
heapsort(a, n)
local ok = true
for i = 2, n do if a[i - 1] > a[i] then ok = false end end
print(ok, string.format("%d", a[1]), string.format("%d", a[n]))
