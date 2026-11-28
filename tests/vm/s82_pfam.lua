-- Section 82.1 (`pfam`): per-iterator trace families at generic-`for` heads. One `reduce` head serves many pipelines (different iterator
-- prototypes, the same prototype over different upvalue closures, nested), C-function iterators share a head with Lua closures, a callable
-- table, fresh closures every round under collections (reused addresses), the iterator replaced mid-loop by `debug.setlocal`, a yield and an
-- error from inside the iterator.

local function range(a, b) local i = a - 1 return function() i = i + 1 if i <= b then return i end end end
local function map(src, f) return function() local v = src() if v ~= nil then return f(v) end end end
local function filter(src, p)
  return function()
    while true do
      local v = src()
      if v == nil then return nil end
      if p(v) then return v end
    end
  end
end
local function take(src, n) local c = 0 return function() if c >= n then return nil end c = c + 1 return src() end end
local function zip(a, b) return function() local x, y = a(), b() if x ~= nil and y ~= nil then return x * 1000 + y end end end
local function reduce(src, f, acc) for v in src do acc = f(acc, v) end return acc end
local function add(a, b) return a + b end
local function cnt(n) return n + 1 end

-- 1. one head, many pipelines, in phases
for phase = 1, 5 do
  local out = {}
  out[#out + 1] = reduce(map(range(1, 2000), function(x) return x * 2 end), add, 0)
  out[#out + 1] = reduce(filter(range(1, 3000), function(x) return x % 7 == 3 end), add, 0)
  out[#out + 1] = reduce(take(map(range(1, 100000), function(x) return x % 13 end), 1500), add, 0)
  out[#out + 1] = reduce(zip(range(1, 900), map(range(1, 900), function(x) return x % 5 end)), add, 0)
  out[#out + 1] = reduce(map(filter(range(1, 2500), function(x) return x % 3 == 0 end), function(x) return -x end), cnt, 0)
  out[#out + 1] = reduce(map(zip(range(1, 700), range(5, 800)), function(x) return x % 97 end), add, 0)
  -- nested: the iterator's function runs the same head
  out[#out + 1] = reduce(map(range(1, 40), function(i) return reduce(map(range(1, i), function(j) return i * j end), add, 0) end), add, 0)
  print("phase", phase, table.concat(out, " "))
end

-- 2. C functions and Lua closures through one generic-for head
local function fold3(f, s, c, acc)
  for k, v in f, s, c do acc = acc + (type(v) == "number" and v or 1) + (type(k) == "number" and k or 0) end
  return acc
end
local t = {}
for i = 1, 300 do t[i] = i * 3 end
local h = { a = 1, b = 2, c = 3, d = 4 }
for r = 1, 40 do
  local f1, s1, c1 = ipairs(t)
  local x = fold3(f1, s1, c1, 0)
  local y = fold3(next, h, nil, 0)
  local z = fold3(function(_, k) k = k + 1 if k <= 200 then return k, k * 2 end end, nil, 0, 0)
  local f2, s2, c2 = pairs(t)
  local w = fold3(f2, s2, c2, 0)
  if r % 10 == 0 then print("fold3", r, x, y, z, w) end
end

-- 3. a callable table as the iterator
local callable = setmetatable({ n = 0 }, { __call = function(self) self.n = self.n + 1 if self.n <= 500 then return self.n end end })
print("callable", reduce(callable, add, 0))

-- 4. fresh closures every round, collections in between (iterator addresses are reused)
local acc = 0
for r = 1, 200 do
  local k = r % 4
  local src
  if k == 0 then src = range(1, 50)
  elseif k == 1 then src = map(range(1, 50), function(x) return x + r end)
  elseif k == 2 then src = filter(range(1, 80), function(x) return x % 2 == 0 end)
  else src = take(range(1, 1000), 40) end
  acc = acc + reduce(src, add, 0)
  if r % 16 == 0 then collectgarbage() end
end
print("fresh", acc)

-- 5. the iterator replaced mid-loop (debug.setlocal on the hidden iterator variable)
local function swapping(src, other, at)
  local s = 0
  for v in src do
    s = s + v
    if v == at then
      local i = 1
      while true do
        local name = debug.getlocal(1, i)
        if name == nil then break end
        if name == "(for state)" then debug.setlocal(1, i, other) break end
        i = i + 1
      end
    end
  end
  return s
end
for r = 1, 30 do
  local a = swapping(range(1, 300), map(range(1, 100), function(x) return x * 10 end), 150)
  local b = swapping(map(range(1, 300), function(x) return x + 1 end), range(1000, 1100), 151)
  if r % 10 == 0 then print("swap", r, a, b) end
end

-- 6. a yield and an error from inside the iterator
local co = coroutine.wrap(function()
  local s = 0
  for r = 1, 30 do
    s = s + reduce(map(range(1, 100), function(x) if x == 50 then coroutine.yield(x) end return x end), add, 0)
  end
  return s
end)
local ys = 0
while true do
  local v = co()
  if v == 50 then ys = ys + 1 else print("yields", ys, v) break end
end
for r = 1, 30 do
  local ok, err = pcall(reduce, map(range(1, 200), function(x) if x == 77 + r then error("boom " .. x) end return x end), add, 0)
  if r % 10 == 0 then print("error", r, ok, err) end
end
