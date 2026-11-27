-- Sections 79.3 / 79.4 (`rootproto`, `mfside`): the root frame's own closure takes no prototype guard; E5's fold of the family key in frames that
-- `mfdeep` does not cover (side traces' seeded chain frames, inlined frames called with another closure ref) under a receiver guard.
-- (1) permute (its side traces start inside chain frames); (2) an inlined frame of the root prototype on another receiver whose class wraps
-- the method; (3) the method replaced mid-run, from outside and from inside the recursion; (4) a callee read from a field that is the root
-- closure at record time and another function later; (5) setmetatable swapping the class of the receiver between runs.
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
for k = 1, 6 do
  local p = setmetatable({ count = 0, v = { 1, 2, 3, 4, 5, 6 } }, P)
  p:permute(5)
  print("permute", k, p.count, table.concat(p.v, ","))
end

-- (2) `R.run(self.other, n - 1)` passes fewer arguments than parameters: inlined, not pushed; in that frame `self:run` finds `O.run`
local R = {}
R.__index = R
function R:run(n, extra)
  self.c = self.c + 1
  if n > 0 then
    for i = 1, 2 do
      if i == 2 and n % 3 == 0 and self.other then
        R.run(self.other, n - 1)
      else
        self:run(n - 1, i)
      end
    end
  end
  return extra
end
local O = setmetatable({}, { __index = R })
O.__index = O
function O:run(n, extra)
  self.w = self.w + 1
  return R.run(self, n, extra)
end
for k = 1, 4 do
  local other = setmetatable({ c = 0, w = 0 }, O)
  local r = setmetatable({ c = 0, w = 0, other = other }, R)
  r:run(9)
  print("other", k, r.c, r.w, other.c, other.w)
end

-- (2b) the permute shape with an explicit call `T.walk(t, n1, 0)` (three arguments for two parameters: inlined, not pushed) whose receiver is
-- the frame's own at first and another object (whose class wraps `walk`) later
local T = {}
T.__index = T
function T:walk(n)
  self.c = self.c + 1
  if n ~= 0 then
    local n1 = n - 1
    self:walk(n1)
    for i = n1, 0, -1 do
      local t = self
      if self.k > 40 and i == 0 then t = self.other end
      self.k = self.k + 1
      T.walk(t, n1, 0)
    end
  end
end
local U = setmetatable({}, { __index = T })
U.__index = U
function U:walk(n)
  self.w = self.w + 1
  return T.walk(self, n)
end
for k = 1, 4 do
  local other = setmetatable({ c = 0, w = 0, k = 0 }, U)
  other.other = other
  local t = setmetatable({ c = 0, w = 0, k = 0, other = other }, T)
  t:walk(5)
  print("other2", k, t.c, t.w, other.c, other.w)
end

-- (3) the method replaced between runs and from inside the recursion (depth 3 of run 3)
local M = {}
M.__index = M
function M:go(n)
  self.c = self.c + 1
  if n > 0 then
    for i = 1, 2 do
      self:go(n - 1)
      if self.flip and n == 3 then
        self.flip = false
        local old = M.go
        M.go = function(s, m) s.alt = s.alt + 1 return old(s, m) end
      end
    end
  end
end
local orig = M.go
for k = 1, 5 do
  local m = setmetatable({ c = 0, alt = 0, flip = (k == 3) }, M)
  m:go(8)
  print("replace", k, m.c, m.alt)
  if k == 4 then M.go = orig end
end

-- (4) a callee read from a field: the root closure at record time, another function later
local F = {}
F.__index = F
function F:rec(n)
  self.c = self.c + 1
  if n > 0 then
    for i = 1, 2 do
      local f = self.fn
      f(self, n - 1)
    end
  end
end
for k = 1, 4 do
  local t = setmetatable({ c = 0, d = 0 }, F)
  t.fn = F.rec
  if k == 3 then
    t.fn = function(s, n) s.d = s.d + 1 if n > 0 then F.rec(s, n - 1) end end
  end
  t:rec(8)
  print("field", k, t.c, t.d)
end

-- (5) setmetatable swapping the class of the receiver between runs (a subclass with its own swap)
local Sub = setmetatable({}, { __index = P })
Sub.__index = Sub
function Sub:swap(i, j)
  self.subs = self.subs + 1
  P.swap(self, i, j)
end
for k = 1, 4 do
  local p = setmetatable({ count = 0, subs = 0, v = { 1, 2, 3, 4, 5, 6 } }, (k % 2 == 0) and Sub or P)
  p:permute(4)
  print("class", k, p.count, p.subs, table.concat(p.v, ","))
end
