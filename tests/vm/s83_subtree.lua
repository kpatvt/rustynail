-- Section 83 (subtree recording): a recursive method's call subtree inlined into one trace (`recinl` chain frames that return and call
-- again, their loops unrolled by `chainloop`), loop traces of lazy-aware families entered by native returns (`lzinner`), pre-roll exits
-- linked directly (`predl`). Everything that can observe the frames of the inlined levels or of the lazy frames below a loop trace:
-- (1) permute at several sizes (levels other than the recorded one leave through guards of the inlined levels);
-- (2) an error raised at a leaf of the subtree, caught at several depths, with the traceback and the error position;
-- (3) debug.getinfo / debug.traceback at the leaves (the frames below a loop trace entered by a native return);
-- (4) a coroutine that yields at the leaves of the subtree and is resumed from outside;
-- (5) a count hook installed from inside the recursion after the traces exist;
-- (6) stack overflow of a subtree recursion with loops (message and depth);
-- (7) allocation and full collections at the leaves, finalizers that run between levels;
-- (8) levels with varying loop trip counts (the unrolled copies leave through their count guards), early returns from the loop.
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
for _, size in ipairs({ 6, 3, 5, 2, 6, 4, 1, 6 }) do
  local p = setmetatable({ count = 0, v = { 1, 2, 3, 4, 5, 6 } }, P)
  p:permute(size)
  print("permute", size, p.count, table.concat(p.v, ","))
end

-- (2) an error at a leaf of the subtree; pcall at the top and at an inner level
local E = {}
E.__index = E
function E:walk(n)
  self.c = self.c + 1
  if n == 0 then
    self.l = (self.l or 0) + 1
    if self.l == self.limit then error("leaf " .. self.c) end
    return
  end
  local n1 = n - 1
  self:walk(n1)
  for i = n1, 0, -1 do
    self:walk(n1)
  end
end
function E:guarded(n)
  local ok, msg = pcall(self.walk, self, n)
  return ok, msg
end
for k = 1, 6 do
  local e = setmetatable({ c = 0, limit = 40 * k + 7 }, E)
  local ok, msg = pcall(E.walk, e, 5)
  print("error", k, ok, msg, e.c)
  local e2 = setmetatable({ c = 0, limit = 30 * k + 3 }, E)
  print("inner", k, e2:guarded(4), e2.c)
  local e3 = setmetatable({ c = 0, limit = 25 * k }, E)
  local ok3, tb = xpcall(E.walk, debug.traceback, e3, 4)
  local lines = 0
  for _ in tostring(tb):gmatch("walk") do lines = lines + 1 end
  print("traceback", k, ok3, lines, tostring(tb):match("^[^\n]*"))
end

-- (3) getinfo / traceback at the leaves: the depth of the frame and the current lines of the levels below
local G = {}
G.__index = G
local gbase = 0
function G:depthof()
  local d = 1
  while debug.getinfo(d, "l") do d = d + 1 end
  return d
end
function G:walk(n)
  self.c = self.c + 1
  if n == 0 then
    if self.c % 97 == 0 then
      local info = debug.getinfo(3, "nSl")
      self.log[#self.log + 1] = (self:depthof() - gbase) .. ":" .. tostring(info.currentline) .. ":" .. tostring(info.name)
    end
    return
  end
  local n1 = n - 1
  self:walk(n1)
  for i = n1, 0, -1 do
    self:walk(n1)
  end
end
gbase = G.depthof()
for k = 1, 3 do
  local g = setmetatable({ c = 0, log = {} }, G)
  g:walk(5)
  print("getinfo", k, g.c, #g.log, table.concat(g.log, " "))
end

-- (4) yields at the leaves
local Y = {}
Y.__index = Y
function Y:walk(n)
  self.c = self.c + 1
  if n == 0 then
    if self.c % 13 == 0 then coroutine.yield(self.c) end
    return
  end
  local n1 = n - 1
  self:walk(n1)
  for i = n1, 0, -1 do
    self:walk(n1)
  end
end
for k = 1, 3 do
  local y = setmetatable({ c = 0 }, Y)
  local co = coroutine.wrap(function() y:walk(5) return "done" end)
  local got, sum = 0, 0
  while true do
    local v = co()
    if v == "done" then break end
    got = got + 1
    sum = sum + v
  end
  print("yield", k, y.c, got, sum)
end

-- (5) a count hook installed from inside the recursion once the traces exist
local H = {}
H.__index = H
local hooks = 0
function H:walk(n)
  self.c = self.c + 1
  if n == 0 then
    if self.c == self.at then debug.sethook(function() hooks = hooks + 1 end, "", 1000) end
    return
  end
  local n1 = n - 1
  self:walk(n1)
  for i = n1, 0, -1 do
    self:walk(n1)
  end
end
for k = 1, 3 do
  local h = setmetatable({ c = 0, at = 900 + k }, H)
  h:walk(5)
  debug.sethook()
  print("hook", k, h.c, hooks > 0)
  hooks = 0
end

-- (6) stack overflow of a recursion with loops (a level that never ends)
local O = {}
O.__index = O
function O:dive(n)
  self.c = self.c + 1
  for i = 1, (n % 2) + 1 do
    if i == 2 then return end
    self:dive(n + 1)
  end
end
for k = 1, 2 do
  local o = setmetatable({ c = 0 }, O)
  local ok, msg = pcall(O.dive, o, 0)
  print("overflow", k, ok, (tostring(msg):gsub("^.-:%d+: ", "")), o.c > 1000)
end

-- (7) allocation, collections and finalizers at the leaves
local A = {}
A.__index = A
local fin = 0
function A:walk(n)
  self.c = self.c + 1
  if n == 0 then
    self.keep[#self.keep % 7 + 1] = { self.c, tostring(self.c) }
    if self.c % 61 == 0 then
      setmetatable({}, { __gc = function() fin = fin + 1 end })
      collectgarbage()
    end
    return
  end
  local n1 = n - 1
  self:walk(n1)
  for i = n1, 0, -1 do
    self:walk(n1)
  end
end
for k = 1, 3 do
  local a = setmetatable({ c = 0, keep = {} }, A)
  a:walk(5)
  collectgarbage()
  print("alloc", k, a.c, #a.keep, a.keep[1][2], fin > 0)
end

-- (8) varying trip counts per level and early returns from the loop
local V = {}
V.__index = V
function V:walk(n, w)
  self.c = self.c + 1
  if n == 0 then return w end
  local s = 0
  for i = 1, (n * w) % 4 + 1 do
    s = s + self:walk(n - 1, (w + i) % 5)
    if s > 40 + n then return s end
  end
  return s
end
for k = 1, 5 do
  local v = setmetatable({ c = 0 }, V)
  print("vary", k, v:walk(6, k), v.c)
end
