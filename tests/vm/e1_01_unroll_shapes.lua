-- E1 (up-recursion unrolling, docs/JIT_TRACE.md section 34): recursion shapes whose function-entry traces inline their self-call
-- k - 1 times and push k lazy frames. Every result must equal the interpreter's.

-- tree recursion through the function's own upvalue (the unrolled shape)
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local s = 0
for i = 1, 30 do s = s + fib(i % 18) end
print("fib", s, fib(20))

-- linear recursion, a leaf at the bottom only
local function sum(n) if n == 0 then return 0 end return n + sum(n - 1) end
local t = 0
for i = 1, 400 do t = t + sum(i % 120) end
print("sum", t)

-- allocation in every frame (the leaves return a fresh table)
local function tree(d) if d == 0 then return {} end return { tree(d - 1), tree(d - 1) } end
local function check(n) if n[1] == nil then return 1 end return 1 + check(n[1]) + check(n[2]) end
local c = 0
for i = 1, 60 do c = c + check(tree(i % 9)) end
print("tree", c)

-- the base case on the other branch, two arguments, a float result
local function pw(x, n) if n > 0 then return x * pw(x, n - 1) end return 1.0 end
local p = 0
for i = 1, 200 do p = p + pw(1.01, i % 40) end
print("pow", string.format("%.6f", p))

-- mutual recursion (the self-call shape breaks after one level: no unrolling across prototypes)
local even, odd
function even(n) if n == 0 then return true end return odd(n - 1) end
function odd(n) if n == 0 then return false end return even(n - 1) end
local e = 0
for i = 1, 300 do if even(i % 77) then e = e + 1 end end
print("mutual", e)

-- tail recursion (the root TAILCALL: no native push) and a non-tail self-call in the same function
local function acc(n, a) if n == 0 then return a end return acc(n - 1, a + n) end
local function mix(n) if n < 3 then return n end local x = mix(n - 1) return acc(n, x) % 1000 end
local m = 0
for i = 1, 200 do m = m + mix(i % 25) end
print("tail", m)

-- vararg recursion (not lazy-aware) next to a fixed-arity one
local function va(n, ...) if n == 0 then return select("#", ...) end return va(n - 1, n, ...) end
local function cnt(n) if n == 0 then return 0 end return 1 + cnt(n - 1) end
local v = 0
for i = 1, 100 do v = v + va(i % 10) + cnt(i % 50) end
print("vararg", v)

-- closures captured in the recursion, upvalues read at every level
local function mk(base)
  local function r(n) if n == 0 then return base end return r(n - 1) + base + n end
  return r
end
local q = 0
local rs = { mk(1), mk(2), mk(3) }
for i = 1, 300 do q = q + rs[i % 3 + 1](i % 30) end
print("closures", q)

-- method recursion (callee found through a table: not unrolled by the shape rule)
local T = {}
T.__index = T
function T:depth(n) if n == 0 then return self.k end return self:depth(n - 1) + 1 end
local o = setmetatable({ k = 5 }, T)
local md = 0
for i = 1, 300 do md = md + o:depth(i % 40) end
print("method", md)

-- a path that changes with the data: deep and shallow invocations interleaved
local function ack(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack(m - 1, 1) end
  return ack(m - 1, ack(m, n - 1))
end
print("ack", ack(2, 3), ack(2, 9), ack(3, 3))

-- recursion that returns in different frames with different counts of work
local function walk(n) if n <= 1 or n % 7 == 0 then return 1 end if n % 2 == 0 then return 1 + walk(n - 1) end return walk(n - 1) + walk(n - 3) end
local w = 0
for i = 1, 200 do w = w + walk(i % 30 + 1) end
print("walk", w)
