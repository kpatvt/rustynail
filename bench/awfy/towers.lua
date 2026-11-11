-- Towers: towers of Hanoi with linked disks (port from the public are-we-fast-yet "Towers" description).
-- Verification: 8191 moves for 13 disks per iteration.  Plain Lua 5.1-compatible; runs unchanged on
-- PUC 5.4/5.5 and LuaJIT.
local N = tonumber(arg and arg[1]) or 45

local Disk = {}
Disk.__index = Disk
local function newDisk(size) return setmetatable({ size = size, nxt = nil }, Disk) end

local Towers = {}
Towers.__index = Towers
function Towers:pushDisk(disk, pile)
  local top = self.piles[pile]
  if top ~= nil and disk.size >= top.size then error("Cannot put a big disk on a smaller one") end
  disk.nxt = top
  self.piles[pile] = disk
end
function Towers:popDiskFrom(pile)
  local top = self.piles[pile]
  if top == nil then error("Attempting to remove a disk from an empty pile") end
  self.piles[pile] = top.nxt
  top.nxt = nil
  return top
end
function Towers:moveTopDisk(from, to)
  self:pushDisk(self:popDiskFrom(from), to)
  self.movesDone = self.movesDone + 1
end
function Towers:buildTowerAt(pile, disks)
  for i = disks, 0, -1 do self:pushDisk(newDisk(i), pile) end
end
function Towers:moveDisks(disks, from, to)
  if disks == 1 then
    self:moveTopDisk(from, to)
  else
    local other = 3 - from - to
    self:moveDisks(disks - 1, from, other)
    self:moveTopDisk(from, to)
    self:moveDisks(disks - 1, other, to)
  end
end

local total, ok = 0, 0
for it = 1, N do
  local t = setmetatable({ piles = {}, movesDone = 0 }, Towers)
  t:buildTowerAt(1, 13)
  t:moveDisks(13, 1, 2)
  total = total + t.movesDone
  if t.movesDone == 8191 then ok = ok + 1 end
end
print("towers", ok, total)
