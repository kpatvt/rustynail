//! T7 programs (docs/JIT_TRACE.md section 48.6): function-entry roots of short non-recursive functions (`t7`: a head that `fnpromo` promoted records
//! bodies shorter than `fnmin`). The towers-of-Hanoi shape (leaf methods of 10 to 15 bytecodes called from a recursion and from loops), a table of short
//! accessors called through several shapes of receiver, a short function whose branches flip (the leaf trace exits and gets side traces).

use super::progs::Prog;

pub const T7_PROGS: &[Prog] = &[
    Prog {
        name: "t7_towers",
        small: false,
        src: r#"
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
local function run(n, d)
  local total = 0
  for it = 1, n do
    local t = setmetatable({ piles = {}, movesDone = 0 }, Towers)
    t:buildTowerAt(1, d)
    t:moveDisks(d, 1, 2)
    total = total + t.movesDone
  end
  return total
end
local out = {}
for r = 1, 6 do out[#out + 1] = run(8, 7 + r % 3) end
-- an error from a leaf method (the error branch of `pushDisk`)
local t = setmetatable({ piles = {}, movesDone = 0 }, Towers)
t:buildTowerAt(1, 3)
out[#out + 1] = tostring(pcall(t.pushDisk, t, newDisk(9), 1))
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "t7_accessors",
        small: false,
        src: r#"
-- short methods (1 to 6 bytecodes) called from untraced and traced code, with receivers of two shapes and a branch that flips
local P = {}
P.__index = P
function P:getx() return self.x end
function P:sety(v) self.y = v end
function P:sum() return self.x + self.y end
function P:pick(c) if c then return self.x end return self.y end
local function mk(i, shape)
  if shape == 0 then return setmetatable({ x = i, y = i * 2 }, P) end
  return setmetatable({ y = i * 2, z = 1, x = i }, P)
end
local function sweep(objs, n)
  local s = 0
  for r = 1, n do
    for i = 1, #objs do
      local o = objs[i]
      o:sety(o:getx() + r)
      s = s + o:sum() + o:pick(i % 3 == 0)
    end
  end
  return s
end
local function viacall(objs, n)
  local s = 0
  for i = 1, n do
    local o = objs[i % #objs + 1]
    s = s + select(2, pcall(o.getx, o)) + o:pick(i % 2 == 0)
  end
  return s
end
local out = {}
for r = 1, 8 do
  local objs = {}
  for i = 1, 20 do objs[i] = mk(i, (i + r) % 2) end
  out[#out + 1] = sweep(objs, 30)
  out[#out + 1] = viacall(objs, 200)
end
print(table.concat(out, " "))
"#,
    },
];
