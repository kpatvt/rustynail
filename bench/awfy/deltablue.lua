-- DeltaBlue: incremental constraint solver (port from the public description of the classic DeltaBlue benchmark,
-- as used by are-we-fast-yet): constraint class hierarchy, planner with incremental add/remove, chain and
-- projection tests.  Plain Lua 5.1-compatible idiom (runs unchanged on PUC 5.4/5.5 and LuaJIT).
-- Output: counts of verified checks (the tests raise an error on any mismatch).
local N = tonumber(arg and arg[1]) or 20
local insert, remove = table.insert, table.remove

local function class(base)
  local c = {}
  c.__index = c
  if base then setmetatable(c, { __index = base }) end
  return c
end

-- Strength ---------------------------------------------------------------
local Strength = class()
local function newStrength(v, name) return setmetatable({ value = v, name = name }, Strength) end
local REQUIRED = newStrength(0, "required")
local STRONG_PREFERRED = newStrength(1, "strongPreferred")
local PREFERRED = newStrength(2, "preferred")
local STRONG_DEFAULT = newStrength(3, "strongDefault")
local NORMAL = newStrength(4, "normal")
local WEAK_DEFAULT = newStrength(5, "weakDefault")
local WEAKEST = newStrength(6, "weakest")
local ALL = { REQUIRED, STRONG_PREFERRED, PREFERRED, STRONG_DEFAULT, NORMAL, WEAK_DEFAULT, WEAKEST }
local function stronger(a, b) return a.value < b.value end
local function weaker(a, b) return a.value > b.value end
local function weakestOf(a, b) if weaker(a, b) then return a else return b end end

local FORWARD, BACKWARD, NONE = 1, -1, 0
local planner
local checks = 0

local function removeValue(t, v)
  for i = 1, #t do
    if t[i] == v then remove(t, i) return end
  end
end

-- Variable ---------------------------------------------------------------
local Variable = class()
local function newVariable(name, value)
  return setmetatable({ name = name, value = value or 0, constraints = {}, determinedBy = nil, mark = 0,
    walkStrength = WEAKEST, stay = true }, Variable)
end
function Variable:addConstraint(c) insert(self.constraints, c) end
function Variable:removeConstraint(c)
  removeValue(self.constraints, c)
  if self.determinedBy == c then self.determinedBy = nil end
end

-- Constraint base ------------------------------------------------------------
local Constraint = class()
function Constraint:addConstraint()
  self:addToGraph()
  planner:incrementalAdd(self)
end
function Constraint:satisfy(mark)
  self:chooseMethod(mark)
  if not self:isSatisfied() then
    if self.strength == REQUIRED then error("Could not satisfy a required constraint!") end
    return nil
  end
  self:markInputs(mark)
  local out = self:output()
  local overridden = out.determinedBy
  if overridden ~= nil then overridden:markUnsatisfied() end
  out.determinedBy = self
  if not planner:addPropagate(self, mark) then error("Cycle encountered") end
  out.mark = mark
  return overridden
end
function Constraint:destroyConstraint()
  if self:isSatisfied() then planner:incrementalRemove(self) else self:removeFromGraph() end
end
function Constraint:isInput() return false end

-- Unary ------------------------------------------------------------------------
local UnaryConstraint = class(Constraint)
function UnaryConstraint:init(v, strength)
  self.strength = strength
  self.myOutput = v
  self.satisfied = false
  self:addConstraint()
end
function UnaryConstraint:addToGraph()
  self.myOutput:addConstraint(self)
  self.satisfied = false
end
function UnaryConstraint:chooseMethod(mark)
  self.satisfied = (self.myOutput.mark ~= mark) and stronger(self.strength, self.myOutput.walkStrength)
end
function UnaryConstraint:isSatisfied() return self.satisfied end
function UnaryConstraint:markInputs(mark) end
function UnaryConstraint:output() return self.myOutput end
function UnaryConstraint:recalculate()
  self.myOutput.walkStrength = self.strength
  self.myOutput.stay = not self:isInput()
  if self.myOutput.stay then self:execute() end
end
function UnaryConstraint:markUnsatisfied() self.satisfied = false end
function UnaryConstraint:inputsKnown(mark) return true end
function UnaryConstraint:removeFromGraph()
  if self.myOutput ~= nil then self.myOutput:removeConstraint(self) end
  self.satisfied = false
end

local StayConstraint = class(UnaryConstraint)
function StayConstraint:execute() end
local function newStay(v, s) local c = setmetatable({}, StayConstraint) c:init(v, s) return c end

local EditConstraint = class(UnaryConstraint)
function EditConstraint:isInput() return true end
function EditConstraint:execute() end
local function newEdit(v, s) local c = setmetatable({}, EditConstraint) c:init(v, s) return c end

-- Binary -----------------------------------------------------------------------
local BinaryConstraint = class(Constraint)
function BinaryConstraint:init(v1, v2, strength)
  self.strength = strength
  self.v1 = v1
  self.v2 = v2
  self.direction = NONE
  self:addConstraint()
end
function BinaryConstraint:chooseMethod(mark)
  if self.v1.mark == mark then
    if self.v2.mark ~= mark and stronger(self.strength, self.v2.walkStrength) then
      self.direction = FORWARD
    else
      self.direction = NONE
    end
    return
  end
  if self.v2.mark == mark then
    if self.v1.mark ~= mark and stronger(self.strength, self.v1.walkStrength) then
      self.direction = BACKWARD
    else
      self.direction = NONE
    end
    return
  end
  if weaker(self.v1.walkStrength, self.v2.walkStrength) then
    if stronger(self.strength, self.v1.walkStrength) then self.direction = BACKWARD else self.direction = NONE end
  else
    if stronger(self.strength, self.v2.walkStrength) then self.direction = FORWARD else self.direction = BACKWARD end
  end
end
function BinaryConstraint:addToGraph()
  self.v1:addConstraint(self)
  self.v2:addConstraint(self)
  self.direction = NONE
end
function BinaryConstraint:isSatisfied() return self.direction ~= NONE end
function BinaryConstraint:markInputs(mark) self:input().mark = mark end
function BinaryConstraint:input() if self.direction == FORWARD then return self.v1 else return self.v2 end end
function BinaryConstraint:output() if self.direction == FORWARD then return self.v2 else return self.v1 end end
function BinaryConstraint:recalculate()
  local ihn, out = self:input(), self:output()
  out.walkStrength = weakestOf(self.strength, ihn.walkStrength)
  out.stay = ihn.stay
  if out.stay then self:execute() end
end
function BinaryConstraint:markUnsatisfied() self.direction = NONE end
function BinaryConstraint:inputsKnown(mark)
  local i = self:input()
  return i.mark == mark or i.stay or i.determinedBy == nil
end
function BinaryConstraint:removeFromGraph()
  if self.v1 ~= nil then self.v1:removeConstraint(self) end
  if self.v2 ~= nil then self.v2:removeConstraint(self) end
  self.direction = NONE
end

local ScaleConstraint = class(BinaryConstraint)
function ScaleConstraint:addToGraph()
  BinaryConstraint.addToGraph(self)
  self.scale:addConstraint(self)
  self.offset:addConstraint(self)
end
function ScaleConstraint:removeFromGraph()
  BinaryConstraint.removeFromGraph(self)
  if self.scale ~= nil then self.scale:removeConstraint(self) end
  if self.offset ~= nil then self.offset:removeConstraint(self) end
end
function ScaleConstraint:markInputs(mark)
  BinaryConstraint.markInputs(self, mark)
  self.scale.mark = mark
  self.offset.mark = mark
end
function ScaleConstraint:execute()
  if self.direction == FORWARD then
    self.v2.value = self.v1.value * self.scale.value + self.offset.value
  else
    self.v1.value = (self.v2.value - self.offset.value) / self.scale.value
  end
end
function ScaleConstraint:recalculate()
  local ihn, out = self:input(), self:output()
  out.walkStrength = weakestOf(self.strength, ihn.walkStrength)
  out.stay = ihn.stay and self.scale.stay and self.offset.stay
  if out.stay then self:execute() end
end
local function newScale(src, scale, offset, dest, strength)
  local c = setmetatable({ scale = scale, offset = offset }, ScaleConstraint)
  c:init(src, dest, strength)
  return c
end

local EqualityConstraint = class(BinaryConstraint)
function EqualityConstraint:execute() self:output().value = self:input().value end
local function newEquality(a, b, s) local c = setmetatable({}, EqualityConstraint) c:init(a, b, s) return c end

-- Plan / Planner -----------------------------------------------------------------
local Plan = class()
function Plan:execute()
  local v = self.v
  for i = 1, #v do v[i]:execute() end
end

local Planner = class()
local function newPlanner() return setmetatable({ currentMark = 0 }, Planner) end
function Planner:incrementalAdd(c)
  local mark = self:newMark()
  local overridden = c:satisfy(mark)
  while overridden ~= nil do overridden = overridden:satisfy(mark) end
end
function Planner:incrementalRemove(c)
  local out = c:output()
  c:markUnsatisfied()
  c:removeFromGraph()
  local unsatisfied = self:removePropagateFrom(out)
  for sv = 0, 5 do
    local strength = ALL[sv + 1]
    for i = 1, #unsatisfied do
      local u = unsatisfied[i]
      if u.strength == strength then self:incrementalAdd(u) end
    end
  end
end
function Planner:newMark()
  self.currentMark = self.currentMark + 1
  return self.currentMark
end
function Planner:makePlan(sources)
  local mark = self:newMark()
  local plan = setmetatable({ v = {} }, Plan)
  local todo = sources
  while #todo > 0 do
    local c = remove(todo, 1)
    if c:output().mark ~= mark and c:inputsKnown(mark) then
      insert(plan.v, c)
      c:output().mark = mark
      self:addConstraintsConsumingTo(c:output(), todo)
    end
  end
  return plan
end
function Planner:extractPlanFromConstraints(constraints)
  local sources = {}
  for i = 1, #constraints do
    local c = constraints[i]
    if c:isInput() and c:isSatisfied() then insert(sources, c) end
  end
  return self:makePlan(sources)
end
function Planner:addPropagate(c, mark)
  local todo = { c }
  while #todo > 0 do
    local d = remove(todo)
    if d:output().mark == mark then
      self:incrementalRemove(c)
      return false
    end
    d:recalculate()
    self:addConstraintsConsumingTo(d:output(), todo)
  end
  return true
end
function Planner:removePropagateFrom(out)
  out.determinedBy = nil
  out.walkStrength = WEAKEST
  out.stay = true
  local unsatisfied = {}
  local todo = { out }
  while #todo > 0 do
    local v = remove(todo)
    local cs = v.constraints
    for i = 1, #cs do
      if not cs[i]:isSatisfied() then insert(unsatisfied, cs[i]) end
    end
    local determining = v.determinedBy
    for i = 1, #cs do
      local nxt = cs[i]
      if nxt ~= determining and nxt:isSatisfied() then
        nxt:recalculate()
        insert(todo, nxt:output())
      end
    end
  end
  return unsatisfied
end
function Planner:addConstraintsConsumingTo(v, coll)
  local determining = v.determinedBy
  local cc = v.constraints
  for i = 1, #cc do
    local c = cc[i]
    if c ~= determining and c:isSatisfied() then insert(coll, c) end
  end
end

-- Tests ---------------------------------------------------------------------------
local function chainTest(n)
  planner = newPlanner()
  local prev, first, last
  for i = 0, n do
    local v = newVariable("v" .. i)
    if prev ~= nil then newEquality(prev, v, REQUIRED) end
    if i == 0 then first = v end
    if i == n then last = v end
    prev = v
  end
  newStay(last, STRONG_DEFAULT)
  local edit = newEdit(first, PREFERRED)
  local plan = planner:extractPlanFromConstraints({ edit })
  for i = 0, 99 do
    first.value = i
    plan:execute()
    if last.value ~= i then error("Chain test failed.") end
    checks = checks + 1
  end
end

local function change(v, newValue)
  local edit = newEdit(v, PREFERRED)
  local plan = planner:extractPlanFromConstraints({ edit })
  for i = 1, 10 do
    v.value = newValue
    plan:execute()
  end
  edit:destroyConstraint()
end

local function projectionTest(n)
  planner = newPlanner()
  local scale = newVariable("scale", 10)
  local offset = newVariable("offset", 1000)
  local src, dst
  local dests = {}
  for i = 0, n - 1 do
    src = newVariable("src" .. i, i)
    dst = newVariable("dst" .. i, i)
    dests[#dests + 1] = dst
    newStay(src, NORMAL)
    newScale(src, scale, offset, dst, REQUIRED)
  end
  change(src, 17)
  if dst.value ~= 1170 then error("Projection 1 failed") end
  change(dst, 1050)
  if src.value ~= 5 then error("Projection 2 failed") end
  change(scale, 5)
  for i = 0, n - 2 do
    if dests[i + 1].value ~= i * 5 + 1000 then error("Projection 3 failed") end
    checks = checks + 1
  end
  change(offset, 2000)
  for i = 0, n - 2 do
    if dests[i + 1].value ~= i * 5 + 2000 then error("Projection 4 failed") end
    checks = checks + 1
  end
end

for i = 1, N do
  chainTest(100)
  projectionTest(100)
end
print("deltablue", checks)
