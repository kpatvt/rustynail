-- Section 77.17 (`polyhead`): one generic-`for` loop head (`reduce`) serving many pipelines of different iterator closures, some with
-- data-dependent inner loops (ineffective traces), nested (the iterator itself runs `reduce`), in alternating phases.

local function wrap(f) return f end
local function range(a, b) local i = a - 1 return wrap(function() i = i + 1 if i <= b then return i end end) end
local function map(src, f) return wrap(function() local v = src() if v ~= nil then return f(v) end end) end
local function filter(src, p)
  return wrap(function()
    while true do
      local v = src()
      if v == nil then return nil end
      if p(v) then return v end
    end
  end)
end
local function take(src, n) local c = 0 return wrap(function() if c >= n then return nil end c = c + 1 return src() end) end
local function reduce(src, f, acc) for v in src do acc = f(acc, v) end return acc end
local function add(a, b) return a + b end
local function isprime(n)
  if n < 2 then return false end
  for d = 2, math.floor(math.sqrt(n)) do if n % d == 0 then return false end end
  return true
end

for phase = 1, 6 do
  local out = {}
  out[#out + 1] = reduce(map(range(1, 3000), function(x) return x * 2 end), add, 0)
  out[#out + 1] = reduce(take(filter(range(2, 100000), isprime), 300), add, 0)
  out[#out + 1] = reduce(filter(range(1, 4000), function(x) return x % 7 == 3 end), add, 0)
  out[#out + 1] = reduce(map(range(1, 60), function(i) return reduce(map(range(1, i), function(j) return i * j end), add, 0) end), add, 0)
  out[#out + 1] = reduce(take(map(range(1, 100000), function(x) return x % 13 end), 2500), add, 0)
  print("phase", phase, table.concat(out, " "))
end
