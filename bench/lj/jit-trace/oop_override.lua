-- P-A informational: a method overridden after warm-up, a class field changed in the loop, instances of two classes
-- alternating in one loop (the receiver's metatable differs between iterations)
local A = {}; A.__index = A
A.k = 1
function A.new(x) return setmetatable({ x = x }, A) end
function A:val() return self.x + self.k end
local B = setmetatable({}, { __index = A }); B.__index = B
function B.new(x) return setmetatable({ x = x }, B) end
function B:val() return self.x * 2 + self.k end
local n = tonumber(arg and arg[1]) or 3000000
local objs = { A.new(1), B.new(2), A.new(3), B.new(4) }
local acc = 0
for i = 1, n do
  local o = objs[i % 4 + 1]
  acc = (acc + o:val()) % 1000003
  if i == math.floor(n / 2) then A.val = function(self) return self.x - self.k end end
  if i % 1000000 == 0 then A.k = A.k + 1 end
end
print(acc, A.k)
