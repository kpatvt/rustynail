-- E5 (region-scoped method identity, docs/JIT_TRACE.md section 35.4): method recursion `self:m(..)` whose lookup is folded to the running
-- closure inside a lazy region and checked at the region's checked entries. Every case changes what the lookup would find, from inside or
-- outside the recursion, or passes another receiver; the results must be the interpreter's.

local function newclass(parent)
  local C = {}
  C.__index = C
  if parent then setmetatable(C, { __index = parent }) end
  return C
end

-- (1) permute and towers shapes: plain method recursion, one class
local P = newclass()
function P:permute(n)
  self.count = self.count + 1
  if n ~= 0 then
    local n1 = n - 1
    self:permute(n1)
    for i = n1, 0, -1 do self:permute(n1) end
  end
end
local pc = 0
for it = 1, 20 do local p = setmetatable({ count = 0 }, P) p:permute(5) pc = pc + p.count end
print("permute", pc)

local T = newclass()
function T:move(n, a, b)
  if n == 1 then self.moves = self.moves + 1 return end
  local o = 6 - a - b
  self:move(n - 1, a, o)
  self.moves = self.moves + 1
  self:move(n - 1, o, b)
end
local tm = 0
for it = 1, 20 do local t = setmetatable({ moves = 0 }, T) t:move(10, 1, 2) tm = tm + t.moves end
print("towers", tm)

-- (2) inheritance: the method is found two and three levels up the __index chain
local Base = newclass()
function Base:depth(n) if n == 0 then return 1 end local s = 0 for i = 1, 2 do s = s + self:depth(n - 1) end return s end
local Mid = newclass(Base)
local Leaf = newclass(Mid)
local d = 0
for it = 1, 30 do d = d + setmetatable({}, Leaf):depth(6) + setmetatable({}, Mid):depth(5) end
print("inherit", d)

-- (3) some instances override the method: their own field is found first (an outer entry on such a receiver must not run the folded code)
local A = newclass()
function A:sum(n) if n == 0 then return self.v end return self:sum(n - 1) + self:sum(n - 1) end
local over = function(self, n) return 1000 end
local s3 = 0
for it = 1, 60 do
  local o = setmetatable({ v = it }, A)
  if it % 7 == 0 then o.sum = over end
  s3 = s3 + o:sum(5)
end
print("override", s3)

-- (4) a subclass replaces the method; the recursion passes instances of both classes through the same call sites
local B = newclass(A)
function B:sum(n) if n == 0 then return -self.v end return self:sum(n - 1) + 1 end
local s4 = 0
for it = 1, 60 do
  local o = setmetatable({ v = it }, (it % 3 == 0) and B or A)
  s4 = s4 + o:sum(4)
end
print("subclass", s4)

-- (5) the class method rebound by a leaf of the recursion (inside the region), then restored
local R = newclass()
local orig
local calls5 = 0
local function alt(self, n) calls5 = calls5 + 1 return 7 end
function R:go(n)
  if n == 0 then
    self.k = self.k + 1
    if self.k % 50 == 0 then R.go = alt end
    return 1
  end
  return self:go(n - 1) + self:go(n - 1)
end
orig = R.go
local s5 = 0
for it = 1, 40 do
  local o = setmetatable({ k = it }, R)
  s5 = s5 + o:go(5)
  R.go = orig
end
print("rebind", s5, calls5)

-- (6) the metatable of the receiver replaced by a leaf through setmetatable, and __index of the class replaced
local S1, S2 = newclass(), newclass()
function S1:f(n) if n == 0 then return 1 end if self.flip and n == 2 then setmetatable(self, S2) end return self:f(n - 1) + 1 end
function S2:f(n) if n == 0 then return 100 end return self:f(n - 1) + 2 end
local s6 = 0
for it = 1, 40 do s6 = s6 + setmetatable({ flip = (it % 5 == 0) }, S1):f(6) end
local U = {}
local Umt = { __index = U }
function U:f(n) if n == 0 then return 1 end return self:f(n - 1) + 1 end
local V = {}
function V:f(n) if n == 0 then return 50 end return self:f(n - 1) + 5 end
local s6b = 0
for it = 1, 40 do
  if it == 30 then Umt.__index = V end
  s6b = s6b + setmetatable({}, Umt):f(5)
end
print("setmt", s6, s6b)

-- (7) another receiver passed to the same closure: `f(self.peer, n)` (the callee is the running closure, the receiver is not register 0)
local Q = newclass()
local Q2 = newclass()
function Q:walk(n)
  if n == 0 then return self.tag end
  local f = self.walk
  if n % 2 == 0 and self.peer then return f(self.peer, n - 1) + 1 end
  return self:walk(n - 1) + 1
end
function Q2:walk(n) if n == 0 then return 500 end return self:walk(n - 1) + 3 end
local s7 = 0
for it = 1, 50 do
  local peer = setmetatable({ tag = 2 }, (it % 2 == 0) and Q2 or Q)
  s7 = s7 + setmetatable({ tag = 1, peer = peer }, Q):walk(6)
end
print("peer", s7)

-- (8) two closures of one prototype (one class each); an instance calls the other class's method on itself
local function mkcls(bias)
  local C = newclass()
  function C:rec(n) if n == 0 then return bias end local g = self.alt if g and n == 3 then return g(self, n - 1) + 0 end return self:rec(n - 1) + 1 end
  return C
end
local C1, C2 = mkcls(10), mkcls(20)
local s8 = 0
for it = 1, 50 do
  local o = setmetatable({}, C1)
  o.alt = (it % 9 == 0) and C2.rec or C1.rec
  s8 = s8 + o:rec(6)
end
print("closures", s8)

-- (9) register 0 reassigned on a rare path: the method of the new receiver is called
local W = newclass()
local W2 = newclass()
function W:hop(n)
  if n == 0 then return 1 end
  if self.next and n == 3 then self = self.next end
  return self:hop(n - 1) + 1
end
function W2:hop(n) if n == 0 then return 30 end return self:hop(n - 1) + 4 end
local s9 = 0
for it = 1, 60 do
  local nx = (it % 6 == 0) and setmetatable({}, W2) or nil
  s9 = s9 + setmetatable({ next = nx }, W):hop(6)
end
print("reassign", s9)

-- (10) an __index function on the path (no fold) and a class whose method is replaced from outside between runs
local F = setmetatable({}, { __index = function(t, k) return rawget(t, "_" .. k) end })
F._ff = function(self, n) if n == 0 then return 1 end return self:ff(n - 1) + 1 end
local s10 = 0
for it = 1, 30 do s10 = s10 + F:ff(5) end
local G = newclass()
function G:g(n) if n == 0 then return 1 end return self:g(n - 1) + self:g(n - 1) end
local s10b = 0
for it = 1, 40 do
  if it == 25 then local old = G.g function G:g(n) if n == 0 then return 3 end return old(self, n) end end
  s10b = s10b + setmetatable({}, G):g(4)
end
print("indexfn", s10, s10b)

-- (11) register 0 reassigned to another receiver passed in: instances of the same class with two table shapes, later instances of another
-- class with the second shape (the lookup on the new receiver happens in side traces of the frame)
local X = newclass()
local X2 = newclass()
function X:step(n, alt)
  if n == 0 then return 1 end
  if alt then self = alt end
  return self:step(n - 1) + 1
end
function X2:step(n) return 1000 end
local s11 = 0
for it = 1, 120 do
  local alt
  if it <= 40 then alt = setmetatable({}, X)
  elseif it % 3 ~= 0 then alt = setmetatable({ k = 1 }, X)
  else alt = setmetatable({ k = 1 }, X2) end
  s11 = s11 + setmetatable({}, X):step(4, alt)
end
print("reassign2", s11)

-- (12) the class of a waiting receiver changed (through a C call) by the frame it called on another receiver: when that frame returns as the
-- root of its region, the caller's continuation must look the method up again
local Y = newclass()
local Y2 = newclass()
function Y:r(n, victim)
  if victim then pcall(setmetatable, victim, Y2) end
  if n == 0 then return 1 end
  local v = 0
  if self.peer then v = self.peer:r(n - 1, self) end
  return v + self:r(n - 1) + 1
end
function Y2:r(n) return 50 end
local s12 = 0
for it = 1, 80 do
  local p = setmetatable({}, Y)
  local o = setmetatable({ peer = (it % 4 == 0) and p or nil }, Y)
  s12 = s12 + o:r(5)
end
print("rootret", s12)
