-- E4 (lazy frames for recursion families with loops, docs/JIT_TRACE.md section 35): recursive functions whose body has a loop around (or
-- beside) the self-call. Their natively pushed frames are lazy once the first root trace of the prototype is the function-entry trace that ends
-- in the self-call; loops whose first trace is a loop trace stay eager. Every shape must give the interpreter's results.

-- permute (method recursion, the self-call before and inside a numeric for loop)
local P = {}
P.__index = P
local function newP(n) local v = {} for i = 1, n do v[i] = i end return setmetatable({ count = 0, v = v }, P) end
function P:swap(i, j) local t = self.v[i + 1] self.v[i + 1] = self.v[j + 1] self.v[j + 1] = t end
function P:permute(n)
  self.count = self.count + 1
  if n ~= 0 then
    local n1 = n - 1
    self:permute(n1)
    for i = n1, 0, -1 do
      self:swap(n1, i)
      self:permute(n1)
      self:swap(n1, i)
    end
  end
end
local total = 0
for it = 1, 30 do local p = newP(6) p:permute(6) total = total + p.count end
print("permute", total)

-- local-function recursion with the self-call inside the loop body (queens)
local function queens(n)
  local cols, d1, d2, count = {}, {}, {}, 0
  local function place(r)
    if r > n then count = count + 1 return end
    for c = 1, n do
      if not cols[c] and not d1[r + c] and not d2[r - c + n] then
        cols[c], d1[r + c], d2[r - c + n] = true, true, true
        place(r + 1)
        cols[c], d1[r + c], d2[r - c + n] = false, false, false
      end
    end
  end
  place(1)
  return count
end
local qs = {}
for n = 4, 8 do qs[#qs + 1] = queens(n) end
print("queens", table.concat(qs, " "))

-- a tree built in a loop of self-calls, leaves allocate in a loop (storage)
local seed = 74755
local function rnd() seed = (seed * 1309 + 13849) % 65536 return seed end
local cnt
local function build(depth)
  cnt = cnt + 1
  if depth == 1 then local a = {} for i = 1, rnd() % 10 + 1 do a[i] = false end return a end
  local arr = {}
  for i = 1, 4 do arr[i] = build(depth - 1) end
  return arr
end
local tot = 0
for it = 1, 12 do cnt = 0 build(6) tot = tot + cnt end
print("build", tot)

-- the self-call beside the loop: sum of a list of lists, depth first
local function nested(d, w)
  if d == 0 then return { 1, 2, 3 } end
  local t = {}
  for i = 1, w do t[i] = nested(d - 1, w) end
  return t
end
local function sum(t)
  local s = 0
  for i = 1, #t do
    local v = t[i]
    if type(v) == "table" then s = s + sum(v) else s = s + v end
  end
  return s
end
local tree = nested(4, 3)
local ss = 0
for i = 1, 40 do ss = ss + sum(tree) end
print("sum", ss)

-- a while loop and a break around the self-call
local function walk(n, acc)
  if n <= 0 then return acc end
  local i = 0
  while true do
    i = i + 1
    if i > 3 then break end
    acc = walk(n - i, acc + i)
  end
  return acc
end
local ws = 0
for i = 1, 25 do ws = ws + walk(i % 9, 0) end
print("walk", ws)

-- a generic for (pairs / ipairs) around the self-call
local function depthsum(t, d)
  local s = d
  for _, v in ipairs(t) do s = s + depthsum(v, d + 1) end
  return s
end
local function mk(d) local t = {} if d > 0 then for i = 1, 3 do t[i] = mk(d - 1) end end return t end
local mt = mk(5)
local ds = 0
for i = 1, 30 do ds = ds + depthsum(mt, 0) end
print("ipairs", ds)
local function countkeys(t)
  local n = 0
  for k, v in pairs(t) do n = n + 1 if type(v) == "table" then n = n + countkeys(v) end end
  return n
end
local ck = 0
for i = 1, 30 do ck = ck + countkeys(mt) end
print("pairs", ck)

-- float and integer results mixed through the recursion, a loop over the children
local function harm(n, k)
  if n == 0 then return 1 / k end
  local s = 0
  for i = 1, 2 do s = s + harm(n - 1, k + i) end
  return s
end
local hs = 0
for i = 1, 20 do hs = hs + harm(8, 1) end
print("harm", string.format("%.10f", hs))

-- global-function recursion with a loop
function GREC(n)
  if n < 2 then return n end
  local s = 0
  for i = 1, 2 do s = s + GREC(n - i) end
  return s
end
local gs = 0
for i = 1, 15 do gs = gs + GREC(16) end
print("global", gs)

-- a non-recursive helper called in the loop of a recursive function (its own traces may return into the recursive frames)
local function helper(x, y) if x > y then return x - y end return y - x + 1 end
local function hrec(n)
  if n == 0 then return 1 end
  local s = 0
  for i = 1, 3 do s = s + helper(n, i) + hrec(n - 1) end
  return s
end
local hr = 0
for i = 1, 30 do hr = hr + hrec(5) % 1000 end
print("helper", hr)

-- the self upvalue replaced in the middle of a run (callee identity of a lazy family with loops)
local f
local function f0(n) if n == 0 then return 0 end local s = 0 for i = 1, 2 do s = s + f(n - 1) + i end return s end
f = f0
local fr = 0
for i = 1, 30 do fr = fr + f(6) end
local calls = 0
f = function(n) calls = calls + 1 return f0(n) end
for i = 1, 5 do fr = fr + f(5) end
f = f0
fr = fr + f(7)
print("switch", fr, calls)

-- many closures of one prototype with loops, recursing through their own upvalue
local function mkrec(k)
  local g
  g = function(n) if n == 0 then return k end local s = 0 for i = 1, 2 do s = s + g(n - 1) end return s end
  return g
end
local gr = 0
for k = 1, 8 do local g = mkrec(k) for i = 1, 5 do gr = gr + g(6) end end
print("closures", gr)
