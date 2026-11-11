-- Queens: eight queens, first solution by backtracking with boolean arrays (port from the public
-- are-we-fast-yet "Queens" description). Verification: 10 solutions per iteration, the same row vector each time.
-- Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
local N = tonumber(arg and arg[1]) or 200

local Q = {}
Q.__index = Q
function Q:getRowColumn(r, c)
  return self.freeRows[r + 1] and self.freeMaxs[c + r + 1] and self.freeMins[c - r + 8]
end
function Q:setRowColumn(r, c, v)
  self.freeRows[r + 1] = v
  self.freeMaxs[c + r + 1] = v
  self.freeMins[c - r + 8] = v
end
function Q:placeQueen(c)
  for r = 0, 7 do
    if self:getRowColumn(r, c) then
      self.queenRows[r + 1] = c
      self:setRowColumn(r, c, false)
      if c == 7 then return true end
      if self:placeQueen(c + 1) then return true end
      self:setRowColumn(r, c, true)
    end
  end
  return false
end
local function queens()
  local q = setmetatable({ freeRows = {}, freeMaxs = {}, freeMins = {}, queenRows = {} }, Q)
  for i = 1, 8 do q.freeRows[i] = true; q.queenRows[i] = -1 end
  for i = 1, 16 do q.freeMaxs[i] = true; q.freeMins[i] = true end
  local ok = q:placeQueen(0)
  local sig = 0
  for i = 1, 8 do sig = sig * 10 + q.queenRows[i] end
  return ok, sig
end

local good, sig = 0, 0
for it = 1, N do
  for i = 1, 10 do
    local ok, s = queens()
    if ok then good = good + 1 end
    sig = s
  end
end
print("queens", good, sig)
