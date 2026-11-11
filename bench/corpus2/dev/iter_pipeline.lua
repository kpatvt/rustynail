-- Functional-programming helpers (map/filter/reduce/zip/take/groupBy) and a small
-- luafun-style lazy iterator library with method chaining. small: N=5
local N = tonumber(arg and arg[1]) or 90000

local unpack = table.unpack or unpack

-- Eager helpers ----------------------------------------------------------------------
local F = {}

function F.map(t, f)
  local r = {}
  for i = 1, #t do r[i] = f(t[i], i) end
  return r
end

function F.filter(t, pred)
  local r = {}
  for i = 1, #t do
    if pred(t[i]) then r[#r + 1] = t[i] end
  end
  return r
end

function F.reduce(t, f, init)
  local acc = init
  for i = 1, #t do acc = f(acc, t[i]) end
  return acc
end

function F.zip(a, b)
  local r = {}
  for i = 1, math.min(#a, #b) do r[i] = { a[i], b[i] } end
  return r
end

function F.take(t, n)
  local r = {}
  for i = 1, math.min(n, #t) do r[i] = t[i] end
  return r
end

function F.groupBy(t, keyf)
  local groups, keys = {}, {}
  for _, v in ipairs(t) do
    local k = keyf(v)
    if not groups[k] then groups[k] = {}; keys[#keys + 1] = k end
    table.insert(groups[k], v)
  end
  table.sort(keys)
  return groups, keys
end

function F.compose(...)
  local fns = { ... }
  return function(...)
    local args = { ... }
    for i = #fns, 1, -1 do args = { fns[i](unpack(args)) } end
    return unpack(args)
  end
end

-- Lazy iterators: an iterator is a function returning the next value or nil -------------
local Iter = {}
Iter.__index = Iter

local function wrap(next) return setmetatable({ next = next }, Iter) end

function Iter.range(from, to, step)
  step = step or 1
  local i = from - step
  return wrap(function()
    i = i + step
    if (step > 0 and i <= to) or (step < 0 and i >= to) then return i end
  end)
end

function Iter.from(t)
  local i = 0
  return wrap(function()
    i = i + 1
    return t[i]
  end)
end

function Iter.cycle(t)
  local i = 0
  return wrap(function()
    i = i % #t + 1
    return t[i]
  end)
end

function Iter:map(f)
  local nxt = self.next
  return wrap(function()
    local v = nxt()
    if v ~= nil then return f(v) end
  end)
end

function Iter:filter(pred)
  local nxt = self.next
  return wrap(function()
    while true do
      local v = nxt()
      if v == nil then return nil end
      if pred(v) then return v end
    end
  end)
end

function Iter:take(n)
  local nxt, count = self.next, 0
  return wrap(function()
    if count >= n then return nil end
    count = count + 1
    return nxt()
  end)
end

function Iter:takeWhile(pred)
  local nxt, done = self.next, false
  return wrap(function()
    if done then return nil end
    local v = nxt()
    if v ~= nil and pred(v) then return v end
    done = true
  end)
end

function Iter:drop(n)
  local nxt, dropped = self.next, false
  return wrap(function()
    if not dropped then
      dropped = true
      for _ = 1, n do nxt() end
    end
    return nxt()
  end)
end

function Iter:zip(other)
  local a, b = self.next, other.next
  return wrap(function()
    local x, y = a(), b()
    if x ~= nil and y ~= nil then return { x, y } end
  end)
end

function Iter:enumerate()
  local nxt, i = self.next, 0
  return wrap(function()
    local v = nxt()
    if v ~= nil then i = i + 1; return { i, v } end
  end)
end

function Iter:chain(other)
  local a, b, first = self.next, other.next, true
  return wrap(function()
    if first then
      local v = a()
      if v ~= nil then return v end
      first = false
    end
    return b()
  end)
end

function Iter:reduce(f, init)
  local acc = init
  for v in self.next do acc = f(acc, v) end
  return acc
end

function Iter:totable()
  local t = {}
  for v in self.next do t[#t + 1] = v end
  return t
end

function Iter:count() return self:reduce(function(n) return n + 1 end, 0) end
function Iter:sum() return self:reduce(function(a, b) return a + b end, 0) end
function Iter:each(f) for v in self.next do f(v) end end
function Iter:any(pred) for v in self.next do if pred(v) then return true end end return false end
function Iter:all(pred) for v in self.next do if not pred(v) then return false end end return true end
Iter.__call = function(self) return self.next() end

-- Data ---------------------------------------------------------------------------------------
local seed = 8675309
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

local depts = { "eng", "ops", "sales", "hr", "legal" }
local records = {}
for i = 1, N do
  records[i] = { id = i, dept = depts[rnd(#depts)], age = 20 + rnd(40), salary = 30000 + rnd(900) * 100 }
end

-- Eager pipeline
local adults = F.filter(records, function(r) return r.age >= 30 end)
local salaries = F.map(adults, function(r) return r.salary end)
local total = F.reduce(salaries, function(a, b) return a + b end, 0)
print(string.format("records=%d adults=%d total salary=%d", #records, #adults, total))

local groups, keys = F.groupBy(records, function(r) return r.dept end)
for _, k in ipairs(keys) do
  local g = groups[k]
  local sum = F.reduce(F.map(g, function(r) return r.salary end), function(a, b) return a + b end, 0)
  print(string.format("%-6s n=%4d avg=%9.2f", k, #g, sum / #g))
end

local inc = function(x) return x + 1 end
local dbl = function(x) return x * 2 end
local pipeline = F.compose(inc, dbl, inc)
print("compose(3) = " .. pipeline(3))
local pairsList = F.zip(F.take(salaries, 5), F.map(F.take(adults, 5), function(r) return r.id end))
for _, p in ipairs(pairsList) do io.write(p[2], ":", p[1], " ") end
print()

-- Lazy pipelines
local squares = Iter.range(1, N):map(function(x) return x * x end):filter(function(x) return x % 3 == 1 end)
print("lazy squares sum = " .. squares:take(50):sum())

local primes = Iter.range(2, 100000)
  :filter(function(n)
    for d = 2, math.floor(math.sqrt(n)) do if n % d == 0 then return false end end
    return true
  end)
  :take(math.max(5, math.floor(N / 3)))
local plist = primes:totable()
print(string.format("primes: %d, last=%d, first5=%s", #plist, plist[#plist], table.concat(F.take(plist, 5), ",")))

local fizz = Iter.range(1, 30):map(function(i)
  if i % 15 == 0 then return "FizzBuzz" elseif i % 5 == 0 then return "Buzz" elseif i % 3 == 0 then return "Fizz" end
  return tostring(i)
end)
print(table.concat(fizz:drop(9):take(7):totable(), " "))

local mixed = Iter.from({ "a", "b", "c" }):chain(Iter.cycle({ "x", "y" }):take(4)):enumerate()
mixed:each(function(p) io.write(p[1], "=", p[2], " ") end)
print()

local zipped = Iter.range(1, N):zip(Iter.cycle({ 10, 20, 30 })):map(function(p) return p[1] * p[2] end)
print("zip product sum = " .. zipped:sum())
local ct = Iter.from(records):filter(function(r) return r.dept == "eng" end):map(function(r) return r.salary end):count()
print("eng count = " .. ct)
print("any/all: " .. tostring(Iter.range(1, 10):any(function(x) return x == 7 end)) ..
  " " .. tostring(Iter.range(1, 10):all(function(x) return x < 10 end)))
local tw = Iter.range(1, 1000000):map(dbl):takeWhile(function(x) return x < 2 * N end):count()
print("takeWhile count = " .. tw)
local nested = Iter.range(1, math.max(3, math.floor(N / 20))):map(function(i)
  return Iter.range(1, i):map(function(j) return i * j end):sum()
end):sum()
print("nested sum = " .. nested)
