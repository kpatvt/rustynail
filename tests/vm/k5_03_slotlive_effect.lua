-- K5 (`slotlive`): hot loops whose stores follow a typed load of the same slot (the traces lose the `__newindex` guards of the stores):
-- field updates of a class instance, the permute swap through `self.v`, an in-place array update with a metatable on the array
local C = {}
C.__index = C
function C.new() return setmetatable({ count = 0, step = 3, v = { 1, 2, 3, 4, 5, 6, 7, 8 } }, C) end
function C:tick() self.count = self.count + self.step end
function C:swap(i, j)
  local tmp = self.v[i + 1]
  self.v[i + 1] = self.v[j + 1]
  self.v[j + 1] = tmp
end
local o = C.new()
for i = 1, 2000 do
  o:tick()
  o:swap(i % 8, (i * 3) % 8)
end
print(o.count, table.concat(o.v, ","))
local w = setmetatable({ 0, 0, 0, 0 }, { __index = function() return 0 end })
for i = 1, 2000 do
  local k = i % 4 + 1
  w[k] = w[k] + i
end
print(w[1], w[2], w[3], w[4])
