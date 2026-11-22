-- docs/JIT_TRACE.md section 36.1: the lazy-chain defect of 35.8. A side trace of a lazy-aware recursion family is entered by an exit of its parent
-- inside the parent's region (the natively pushed frames above the region root are still lazy). When the side trace inlines more frames than its
-- parent's entry checks promised room for, it checks the room itself; a failing check used to land in tier 1 at the side's start pc without
-- completing the lazy frames (the interpreter then returned into frames whose `callstatus` / `kcache` were never written). Collections (`shrink_ci`
-- frees half of the unused `CallInfo` nodes) make the spare-node check fail; GC stress makes it fail at almost every entry.

local function newclass()
  local C = {}
  C.__index = C
  return C
end

-- (1) some instances override the method (a second method family runs first, its regions leave the CallInfo list as the next one finds it)
local A = newclass()
function A:sum(n) if n == 0 then return self.v end return self:sum(n - 1) + self:sum(n - 1) end
local over = function(self, n) return 1000 end
local s1 = 0
for it = 1, 60 do
  local o = setmetatable({ v = it }, A)
  if it % 7 == 0 then o.sum = over end
  s1 = s1 + o:sum(5)
end
print("override", s1)

-- (2) register 0 reassigned on a rare path: the side trace of that path inlines the other class's recursion (deeper than its parent)
local W = newclass()
local W2 = newclass()
function W:hop(n)
  if n == 0 then return 1 end
  if self.next and n == 3 then self = self.next end
  return self:hop(n - 1) + 1
end
function W2:hop(n) if n == 0 then return 30 end return self:hop(n - 1) + 4 end
local s2 = 0
for it = 1, 60 do
  local nx = (it % 6 == 0) and setmetatable({}, W2) or nil
  s2 = s2 + setmetatable({ next = nx }, W):hop(6)
end
print("reassign", s2)

-- (3) the same with collections between the recursions and a deeper inlined callee
local s3 = 0
for it = 1, 120 do
  local nx = (it % 4 == 0) and setmetatable({}, W2) or nil
  s3 = s3 + setmetatable({ next = nx }, W):hop(7)
  if it % 3 == 0 then collectgarbage() end
end
print("collect", s3)

-- (4) function recursion through an upvalue: a rare path calls a helper recursion that the side trace inlines
local helper
local function rec(n, flag)
  if n == 0 then return 1 end
  if flag and n == 2 then return helper(n + 1) + rec(n - 1, nil) end
  return rec(n - 1, flag) + 1
end
helper = function(n) if n == 0 then return 5 end return helper(n - 1) + 2 end
local s4 = 0
for it = 1, 150 do
  s4 = s4 + rec(6, it % 5 == 0)
  if it % 10 == 0 then collectgarbage() end
end
print("helper", s4)

-- (5) a tree recursion whose leaves sometimes take a path that calls another method on a peer
local T = newclass()
local P = newclass()
function P:leaf(n) if n == 0 then return 3 end return self:leaf(n - 1) + 1 end
function T:tree(n)
  if n == 0 then
    local p = self.peer
    if p then return p:leaf(3) end
    return 1
  end
  return self:tree(n - 1) + self:tree(n - 1)
end
local s5 = 0
for it = 1, 40 do
  local o = setmetatable({ peer = (it % 3 == 0) and setmetatable({}, P) or nil }, T)
  s5 = s5 + o:tree(5)
  if it % 4 == 0 then collectgarbage() end
end
print("tree", s5)
