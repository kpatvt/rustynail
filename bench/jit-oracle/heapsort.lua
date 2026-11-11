-- Table bucket: array loads, stores and swaps with data-dependent compares (heapsort, repeated).
local function f(a, src, n, reps)
  local acc = 0
  for r = 1, reps do
    for i = 1, n do a[i] = src[i] end
    -- build the heap
    local start = n // 2
    while start >= 1 do
      local root = start
      while true do
        local child = root * 2
        if child > n then break end
        if child < n and a[child] < a[child + 1] then child = child + 1 end
        if a[root] < a[child] then
          a[root], a[child] = a[child], a[root]
          root = child
        else
          break
        end
      end
      start = start - 1
    end
    -- sort down
    local last = n
    while last > 1 do
      a[1], a[last] = a[last], a[1]
      last = last - 1
      local root = 1
      while true do
        local child = root * 2
        if child > last then break end
        if child < last and a[child] < a[child + 1] then child = child + 1 end
        if a[root] < a[child] then
          a[root], a[child] = a[child], a[root]
          root = child
        else
          break
        end
      end
    end
    acc = acc + a[1] + a[n // 2] + a[n]
  end
  return acc
end

local function setup(reps)
  local n = 20000
  local src, a = {}, {}
  local x = 12345
  for i = 1, n do
    x = (x * 1103515245 + 12345) % 2147483648
    src[i] = x
    a[i] = 0
  end
  return a, src, n, reps
end

if ORACLE then return f, setup end
print(f(setup(tonumber(arg and arg[1]) or 50)))
