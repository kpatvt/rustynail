-- Permute: recursive permutation generation of 6 elements (port from the public are-we-fast-yet "Permute"
-- description). Verification: 8660 calls per iteration.
-- Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
local N = tonumber(arg and arg[1]) or 100

local P = {}
P.__index = P
local function newP() return setmetatable({ count = 0, v = { 0, 0, 0, 0, 0, 0 } }, P) end
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

local total, ok = 0, 0
for it = 1, N do
  local p = newP()
  p:permute(6)
  total = total + p.count
  if p.count == 8660 then ok = ok + 1 end
end
print("permute", ok, total)
