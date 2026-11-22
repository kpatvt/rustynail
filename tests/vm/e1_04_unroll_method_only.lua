-- E1 (docs/JIT_TRACE.md section 34.2, the shape rule): method recursion (the callee is found through a table) is not unrolled; this program
-- has no other recursion, so with `recunroll=1` no unrolled trace may be installed. Towers of Hanoi with linked disks (awfy-like).
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
local total = 0
for it = 1, 12 do
  local t = setmetatable({ piles = {}, movesDone = 0 }, Towers)
  for i = 9, 0, -1 do t:pushDisk(newDisk(i), 1) end
  t:moveDisks(9, 1, 2)
  total = total + t.movesDone
end
print("towers", total)
