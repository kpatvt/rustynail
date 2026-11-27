-- Section 79.1 / 79.2 (`chainlive`, `pushtag`): the writeback of a native push leaves out the pushed frame's dead numeric temporaries and the
-- tag stores its SLOADs prove. (1) permute (inlined swap frames leave temporaries where the pushed frame lives); (2) arguments computed in
-- the trace (integers, floats, strings, booleans) with temporaries of a returned callee above them; (3) a slot that changes type between the
-- entry SLOAD and the push; (4) a callee that reads a parameter it was not passed (nil) above stale numeric slots; (5) collectables in the
-- dead slots with full collections in between; (6) varargs callee (never pruned).
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
for k = 1, 5 do
  local p = setmetatable({ count = 0, v = { 1, 2, 3, 4, 5, 6 } }, P)
  p:permute(5)
  print("permute", k, p.count, table.concat(p.v, ","))
end

-- (2) arguments of every type computed in the trace; a returned callee (`mix`) leaves temporaries above the caller's call registers
local W = {}
W.__index = W
function W:mix(a, b)
  local t1 = a * 3 + b
  local t2 = t1 // 2
  local t3 = t2 - a
  self.acc = self.acc + t3
  return t3
end
function W:walk(n, f, s, flag)
  self.n = self.n + 1
  if n > 0 then
    for i = 1, (n % 3) + 1 do
      local m = self:mix(n, i)
      self:walk(n - 1, f * 0.5 + m, s .. (i % 10), not flag)
    end
  else
    self.fs = self.fs + f
    self.len = self.len + #s
    if flag then self.t = self.t + 1 end
  end
end
for k = 1, 4 do
  local w = setmetatable({ n = 0, acc = 0, fs = 0.0, len = 0, t = 0 }, W)
  w:walk(7, k + 0.25, "x", k % 2 == 0)
  print("walk", k, w.n, w.acc, w.fs, w.len, w.t)
end

-- (3) a local that changes type before the push (int -> string -> float), its tag must be stored
local C = {}
C.__index = C
function C:rec(n, x)
  self.c = self.c + 1
  local y = x
  if n % 2 == 0 then y = tostring(x) .. "!" elseif n % 3 == 0 then y = x + 0.5 end
  if n > 0 then
    for i = 1, 2 do
      self:rec(n - 1, i)
    end
  end
  if type(y) == "string" then self.s = self.s + #y elseif math.type(y) == "float" then self.f = self.f + y else self.i = self.i + y end
end
for k = 1, 4 do
  local c = setmetatable({ c = 0, s = 0, f = 0.0, i = 0 }, C)
  c:rec(8, k)
  print("types", k, c.c, c.s, c.f, c.i)
end

-- (4) a callee called with fewer arguments than parameters: the missing one is nil, the slots above it hold stale numbers
local D = {}
D.__index = D
function D:go(n, a, b)
  self.k = self.k + 1
  local z = (a or 7) + (b or 11)
  self.z = self.z + z
  if n > 0 then
    for i = 1, 2 do
      local q = n * i + 0.5
      local r = q * 2
      self.z = self.z + r
      if i == 1 then self:go(n - 1, i) else self:go(n - 1) end
    end
  end
end
for k = 1, 4 do
  local d = setmetatable({ k = 0, z = 0.0 }, D)
  d:go(8)
  print("fewer", k, d.k, d.z)
end

-- (5) collectables next to the dead numeric slots, full collections and a weak table in between
local G = {}
G.__index = G
local weak = setmetatable({}, { __mode = "k" })
function G:keep(t)
  weak[t] = true
  return #t
end
function G:run(n)
  self.r = self.r + 1
  if n > 0 then
    for i = 1, 2 do
      local tmp = { n, i }
      self.s = self.s + self:keep(tmp)
      self:run(n - 1)
    end
  end
  if n == 4 then collectgarbage() end
end
for k = 1, 3 do
  local g = setmetatable({ r = 0, s = 0 }, G)
  g:run(9)
  collectgarbage()
  local live = 0
  for _ in pairs(weak) do live = live + 1 end
  print("gc", k, g.r, g.s, live <= 2048)
end

-- (6) a vararg method recursion (its extra arguments lie above the parameters: never pruned)
local V = {}
V.__index = V
function V:va(n, ...)
  self.c = self.c + select("#", ...)
  local a, b = ...
  self.s = self.s + (a or 0) + (b or 0)
  if n > 0 then
    for i = 1, 2 do
      local t = n + i
      self:va(n - 1, t, i, t * 2)
    end
  end
end
for k = 1, 3 do
  local v = setmetatable({ c = 0, s = 0 }, V)
  v:va(8, k)
  print("vararg", k, v.c, v.s)
end
