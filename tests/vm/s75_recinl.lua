-- Section 75.9 (`recinl`): continuation and loop recordings inline self-calls as an unroll chain, the last one is the native push.
-- (1) permute (method recursion through `__index`, a loop around the self-calls); (2) a tree walk with a loop over children and a
-- recursion that is called with fewer arguments than parameters at some levels; (3) the method replaced mid-run; (4) an error from
-- depth caught by pcall; (5) a field of self changed inside the recursion.
local P = {}
P.__index = P
function P:swap(i, j)
  local tmp = self.v[i + 1]
  self.v[i + 1] = self.v[j + 1]
  self.v[j + 1] = tmp
end
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
for k = 1, 4 do
  local p = setmetatable({ count = 0, v = { 1, 2, 3, 4, 5, 6 } }, P)
  p:permute(5)
  print("permute", k, p.count, table.concat(p.v, ","))
end

local T = {}
T.__index = T
function T:walk(d, acc)
  acc = (acc or 0) + d
  self.n = self.n + 1
  if d > 0 then
    for c = 1, (d % 3) + 1 do
      if c == 2 then
        acc = acc + self:walk(d - 1)          -- fewer arguments: acc is nil in the callee
      else
        acc = acc + self:walk(d - 1, c)
      end
    end
  end
  return acc
end
for k = 1, 3 do
  local t = setmetatable({ n = 0 }, T)
  print("walk", k, t:walk(6, k), t.n)
end

local Q = setmetatable({}, { __index = P })
local q = setmetatable({ count = 0, v = { 1, 2, 3, 4, 5, 6 } }, { __index = Q })
q:permute(4)
local old = P.permute
local calls = 0
Q.permute = function(self, n) calls = calls + 1 return old(self, n) end
q:permute(4)
print("replaced", q.count, calls, table.concat(q.v, ","))
Q.permute = nil

local E = {}
E.__index = E
function E:down(n)
  self.d = self.d + 1
  if n == 0 then error("bottom " .. self.d) end
  for i = 1, 2 do
    if i == 2 and n % 2 == 0 then return self:down(n - 1) end
    self:down(n - 1)
  end
end
for k = 1, 3 do
  local e = setmetatable({ d = 0 }, E)
  local ok, msg = pcall(E.down, e, 6 + k)
  print("error", k, ok, msg, e.d)
end

local F = {}
F.__index = F
function F:grow(n)
  self.total = self.total + n
  if n > 0 then
    for i = 1, 2 do
      self:grow(n - 1)
      if self.total > 300 and not self.flag then self.flag = true; self.v = { 9, 9 } end
    end
  end
end
local f = setmetatable({ total = 0, v = { 1 } }, F)
f:grow(8)
print("grow", f.total, f.flag, #f.v)

-- (6) a vararg method recursion and an open-result self-call ({self:m(...)}) in a loop
local V = {}
V.__index = V
function V:sum(n, ...)
  self.k = self.k + select('#', ...)
  if n == 0 then return 1 end
  local s = 0
  for i = 1, 2 do s = s + self:sum(n - 1, i, ...) end
  return s
end
function V:list(n)
  self.k = self.k + 1
  if n == 0 then return n, n end
  local out = {}
  for i = 1, 2 do
    local r = { self:list(n - 1) }
    out[#out + 1] = #r
  end
  return n, #out, out[1]
end
for k = 1, 3 do
  local v = setmetatable({ k = 0 }, V)
  print("vararg", k, v:sum(7, k), v.k, select('#', v:list(6)), v.k)
end
