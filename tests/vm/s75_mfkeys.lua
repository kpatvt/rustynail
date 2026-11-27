-- Section 75.12 (`mfkeys`): the other method keys of an E5 method family (here `swap`, `bump`) are verified by the region check and kept by the
-- family rule. Each mutation that can change their lookup happens mid-run, between region entries and inside the recursion:
-- the method replaced in the class table, a key assigned into the `__index` table, `setmetatable` swapping the receiver's class, `__index`
-- replaced by a function, the method shadowed on the instance, the method moved one level down an `__index` chain.
local function mkclass(swapname)
  local P = {}
  P.__index = P
  function P:swap(i, j)
    local t = self.v[i + 1]
    self.v[i + 1] = self.v[j + 1]
    self.v[j + 1] = t
  end
  function P:bump() self.b = self.b + 1 end
  function P:permute(n)
    self.count = self.count + 1
    self:bump()
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
  return P
end

local P = mkclass()
local function new(C) return setmetatable({ count = 0, b = 0, v = { 1, 2, 3, 4, 5, 6 } }, C) end
local function show(tag, p) print(tag, p.count, p.b, table.concat(p.v, ",")) end

for k = 1, 3 do local p = new(P) p:permute(5) show("plain", p) end

-- method replaced in the class table between runs
local oldswap = P.swap
P.swap = function(self, i, j) oldswap(self, j, i) self.b = self.b + 2 end
do local p = new(P) p:permute(5) show("replaced", p) end
P.swap = oldswap

-- a key assigned into the __index table from inside the recursion (the family rule must refuse or the check must catch it)
local hits = 0
local Q = mkclass()
local qbump = Q.bump
Q.bump = function(self)
  qbump(self)
  if self.b == 300 then Q.swap = function(s, i, j) hits = hits + 1 oldswap(s, i, j) end end
end
do local q = new(Q) q:permute(5) show("inside", q) print("hits", hits) end

-- setmetatable swaps the receiver's class mid-run (inside the recursion)
local R = mkclass()
local R2 = mkclass()
R2.swap = function(self, i, j) self.b = self.b + 10 end
local rbump = R.bump
R.bump = function(self) rbump(self) if self.b == 200 then setmetatable(self, R2) end end
R2.bump = function(self) self.b = self.b + 1 end
do local r = new(R) r:permute(5) show("setmt", r) end

-- __index replaced by a function between runs
local S = mkclass()
for k = 1, 2 do local s = new(S) s:permute(4) show("s" .. k, s) end
local sraw = {}
for key, val in pairs(S) do sraw[key] = val end
S.__index = function(t, key) return sraw[key] end
do local s = new(S) s:permute(4) show("idxfn", s) end
S.__index = S

-- shadowed on the instance, then moved one level down a chain
do
  local p = new(P)
  p.swap = function(self, i, j) self.b = self.b + 5 end
  p:permute(4)
  show("shadow", p)
  local Base = mkclass()
  local Mid = setmetatable({}, { __index = Base })
  Mid.__index = Mid
  Mid.permute = Base.permute
  Mid.bump = Base.bump
  local m = setmetatable({ count = 0, b = 0, v = { 1, 2, 3, 4, 5, 6 } }, Mid)
  m:permute(4)
  show("chain", m)
  Mid.swap = function(self, i, j) self.b = self.b + 3 end
  m:permute(4)
  show("chain2", m)
end

-- the method shadowed on the instance from inside the recursion (a store of the other key earlier on the chain)
local U = mkclass()
local ubump = U.bump
local uh = 0
U.bump = function(self)
  ubump(self)
  if self.b % 5 == 0 then self.swap = function(s, i, j) uh = uh + 1 end elseif self.b % 5 == 2 then self.swap = nil end
end
do local u = new(U) u:permute(5) show("shadowin", u) print("uh", uh) end

-- the mutations made by the inlined `swap` itself, in a family shaped like awfy/permute (lazy-aware, folded): the method rebound in the class,
-- shadowed on the instance and removed again, the class swapped by setmetatable
local function mkplain()
  local C = {}
  C.__index = C
  function C:permute(n)
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
  return C
end
local function plainswap(self, i, j)
  local t = self.v[i + 1]
  self.v[i + 1] = self.v[j + 1]
  self.v[j + 1] = t
end
local V1 = mkplain()
local vh = 0
local function vshadow(self, i, j) vh = vh + 1 plainswap(self, j, i) end
V1.swap = function(self, i, j)
  plainswap(self, i, j)
  local c = self.count
  if c % 7 == 0 then self.swap = vshadow elseif c % 7 == 3 then self.swap = nil end
end
for k = 1, 3 do local p = new(V1) p:permute(5) show("vshadow", p) end
print("vh", vh)
local V2 = mkplain()
local alt = 0
local function v2b(self, i, j) alt = alt + 1 plainswap(self, j, i) end
local v2a
v2a = function(self, i, j)
  plainswap(self, i, j)
  if self.count % 11 == 0 then V2.swap = v2b end
end
V2.swap = v2a
for k = 1, 3 do local p = new(V2) p:permute(5) show("vrebind", p) V2.swap = v2a end
print("alt", alt)
local V3 = mkplain()
local V3b = mkplain()
V3b.swap = function(self, i, j) plainswap(self, j, i) self.b = self.b + 1 end
V3.swap = function(self, i, j)
  plainswap(self, i, j)
  if self.count % 13 == 0 then setmetatable(self, V3b) end
end
for k = 1, 3 do local p = new(V3) p:permute(5) show("vsetmt", p) end
