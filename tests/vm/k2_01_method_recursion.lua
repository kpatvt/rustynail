-- K2: method recursion (the callee is found through `self`, not through an upvalue) whose entry samples are mostly leaves: permute without a store,
-- a tree walk with a method, a recursive method on subclasses, an overridden method, a method replaced mid-run, a leaf-only method, errors
local P = {}
P.__index = P
local function newP() return setmetatable({ v = { 0, 0, 0, 0, 0 } }, P) end
function P:swap(i, j)
  local tmp = self.v[i + 1]
  self.v[i + 1] = self.v[j + 1]
  self.v[j + 1] = tmp
end
function P:permute(n)
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
for it = 1, 120 do
  local p = newP()
  p:permute(5)
  total = total + p.v[1] + p.v[5] + #p.v
end
print("permute", total)
-- a tree walk through a method with data-dependent leaves
local T = {}
T.__index = T
function T.new(v, l, r) return setmetatable({ v = v, l = l, r = r }, T) end
function T:sum()
  if self.l == nil then return self.v end
  local s = self.l:sum()
  if self.r ~= nil then s = s + self.r:sum() end
  return s + self.v
end
local function build(d, k) if d == 0 then return T.new(k, nil, nil) end return T.new(k, build(d - 1, k + 1), (k % 3 ~= 0) and build(d - 1, k + 2) or nil) end
local tr = build(8, 1)
local acc = 0
for i = 1, 150 do acc = acc + tr:sum() end
print("tree", acc)
-- subclass overriding the recursive method: the callee proto changes with the receiver
local A = {}
A.__index = A
function A:down(n) if n == 0 then return 0 end return 1 + self:down(n - 1) end
local B = setmetatable({}, { __index = A })
B.__index = B
function B:down(n) if n == 0 then return 100 end return 2 + self:down(n - 1) end
local a, b = setmetatable({}, A), setmetatable({}, B)
local s2 = 0
for i = 1, 400 do s2 = s2 + a:down(i % 6) + b:down(i % 4) end
print("sub", s2)
-- the method replaced in the middle of the run
local C = {}
C.__index = C
function C:f(n) if n <= 0 then return 0 end return n + self:f(n - 1) end
local c = setmetatable({}, C)
local s3 = 0
for i = 1, 600 do
  if i == 300 then C.f = function(self, n) if n <= 0 then return 1000 end return n * 2 + self:f(n - 1) end end
  s3 = s3 + c:f(i % 5)
end
print("replaced", s3)
-- a method that calls itself only on a rare path, and one that never does
local D = {}
D.__index = D
function D:rare(n) if n % 40 == 0 then return self:rare(n + 1) + 1 end return n end
function D:never(n) if n < 0 then return math.abs(n) end return n end
local d = setmetatable({}, D)
local s4 = 0
for i = 1, 800 do s4 = s4 + d:rare(i) + d:never(i - 400) end
print("rare", s4)
-- errors inside the recursion, caught by pcall
local E = {}
E.__index = E
function E:go(n) if n == 0 then error("bottom") end if n == 3 and self.bad then error({ n }) end return self:go(n - 1) end
local e1, e2 = setmetatable({}, E), setmetatable({ bad = true }, E)
local errs = 0
for i = 1, 300 do
  local ok, m = pcall(e1.go, e1, i % 6)
  if not ok then errs = errs + #tostring(m) end
  local ok2, m2 = pcall(e2.go, e2, i % 6)
  if not ok2 then errs = errs + (type(m2) == "table" and m2[1] or #m2) end
end
print("errors", errs)
