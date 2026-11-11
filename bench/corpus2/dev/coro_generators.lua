-- Coroutine generators: permutations, tree walkers, a prime sieve built from a chain of filter
-- coroutines, a tokenizer that yields tokens; consumed in generic for loops. small: N=2
local N = tonumber(arg and arg[1]) or 10000

-- Permutations (classic PiL generator) ---------------------------------------------------
local function permgen(a, n)
  n = n or #a
  if n <= 1 then
    coroutine.yield(a)
  else
    for i = 1, n do
      a[n], a[i] = a[i], a[n]
      permgen(a, n - 1)
      a[n], a[i] = a[i], a[n]
    end
  end
end

local function permutations(a)
  local co = coroutine.create(function() permgen(a) end)
  return function()
    local ok, res = coroutine.resume(co)
    return res
  end
end

-- Binary search tree with in-order / level-order walkers ---------------------------------
local Tree = {}
Tree.__index = Tree

local function insert(node, key)
  if not node then return setmetatable({ key = key }, Tree) end
  if key < node.key then node.left = insert(node.left, key)
  elseif key > node.key then node.right = insert(node.right, key) end
  return node
end

function Tree:inorder()
  return coroutine.wrap(function()
    local function walk(n)
      if n then
        walk(n.left)
        coroutine.yield(n.key)
        walk(n.right)
      end
    end
    walk(self)
  end)
end

function Tree:levels()
  return coroutine.wrap(function()
    local queue, head = { self }, 1
    while head <= #queue do
      local n = queue[head]
      head = head + 1
      coroutine.yield(n.key, n)
      if n.left then queue[#queue + 1] = n.left end
      if n.right then queue[#queue + 1] = n.right end
    end
  end)
end

function Tree:height()
  local lh = self.left and self.left:height() or 0
  local rh = self.right and self.right:height() or 0
  return 1 + math.max(lh, rh)
end

-- Prime sieve as a chain of coroutines --------------------------------------------------------
local function counter()
  return coroutine.wrap(function()
    local n = 2
    while true do coroutine.yield(n); n = n + 1 end
  end)
end

local function filter(p, source)
  return coroutine.wrap(function()
    while true do
      local n = source()
      if n % p ~= 0 then coroutine.yield(n) end
    end
  end)
end

local function primes()
  return coroutine.wrap(function()
    local src = counter()
    while true do
      local p = src()
      coroutine.yield(p)
      src = filter(p, src)
    end
  end)
end

-- Tokenizer yielding tokens -----------------------------------------------------------------------
local function tokens(text)
  return coroutine.wrap(function()
    local pos = 1
    while pos <= #text do
      local s, e = text:find("^%s+", pos)
      if s then
        pos = e + 1
      else
        local word = text:match("^%a[%w_]*", pos)
        if word then
          coroutine.yield("word", word)
          pos = pos + #word
        else
          local num = text:match("^%d+", pos)
          if num then
            coroutine.yield("number", tonumber(num))
            pos = pos + #num
          else
            coroutine.yield("punct", text:sub(pos, pos))
            pos = pos + 1
          end
        end
      end
    end
  end)
end

-- Producer / consumer with two-way communication ---------------------------------------------
local function accumulator()
  local co = coroutine.create(function(first)
    local total = first
    while true do
      local v = coroutine.yield(total)
      if v == nil then break end
      total = total + v
    end
    return total
  end)
  return co
end

local function take(n, gen)
  return coroutine.wrap(function()
    for i = 1, n do
      local v = gen()
      if v == nil then return end
      coroutine.yield(v)
    end
  end)
end

-- Driver -----------------------------------------------------------------------------------------------
local perms, checksum = 0, 0
local items = {}
for i = 1, 4 + (N > 10 and 3 or 0) do items[i] = i end
for p in permutations(items) do
  perms = perms + 1
  local code = 0
  for i = 1, #p do code = (code * 7 + p[i]) % 10007 end
  checksum = (checksum + code) % 1000003
end
print(string.format("permutations of %d: %d, checksum %d", #items, perms, checksum))

local seed = 1234
local root
for i = 1, N * 20 do
  seed = (seed * 16807) % 2147483647
  root = insert(root, seed % 10000)
end
local count, sum, last, sorted = 0, 0, -1, true
for key in root:inorder() do
  count = count + 1
  sum = sum + key
  if key <= last then sorted = false end
  last = key
end
print(string.format("tree: %d keys, sum=%d, sorted=%s, height=%d", count, sum, tostring(sorted), root:height()))
local firstLevels = {}
for key, node in root:levels() do
  if #firstLevels < 7 then firstLevels[#firstLevels + 1] = key end
  count = count + (node.left and 1 or 0)
end
print("level order head: " .. table.concat(firstLevels, " "))

local pcount, plast = 0, 0
for rep = 1, math.max(1, math.floor(N / 4)) do
  pcount = 0
  for p in take(30 + rep % 20, primes()) do
    pcount = pcount + 1
    plast = p
  end
end
print(string.format("sieve: %d primes, last=%d", pcount, plast))

local text = string.rep("let x1 = foo(42, bar) + 7;\n", N)
local kinds, wordSum = { word = 0, number = 0, punct = 0 }, 0
for kind, value in tokens(text) do
  kinds[kind] = kinds[kind] + 1
  if kind == "number" then wordSum = wordSum + value end
end
print(string.format("tokens: words=%d numbers=%d punct=%d numsum=%d", kinds.word, kinds.number, kinds.punct, wordSum))

local acc = accumulator()
local _, total = coroutine.resume(acc, 0)
for i = 1, N * 10 do _, total = coroutine.resume(acc, i) end
local ok, final = coroutine.resume(acc, nil)
print(string.format("accumulator: %d final=%d status=%s", total, final, coroutine.status(acc)))

local gens = 0
for round = 1, N * 5 do
  local g = coroutine.wrap(function(a, b)
    local x = coroutine.yield(a + b)
    local y = coroutine.yield(x * 2)
    return x + y
  end)
  local r1 = g(round, 1)
  local r2 = g(r1)
  local r3 = g(r2)
  gens = gens + r3
end
print("wrap round trips: " .. gens)

local ok2, err = pcall(function()
  local co = coroutine.wrap(function() error({ code = 42 }) end)
  co()
end)
print("error propagated: " .. tostring(not ok2) .. " code=" .. tostring(type(err) == "table" and err.code))
