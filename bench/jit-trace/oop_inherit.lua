-- P-A informational: three-level inheritance, methods and class defaults found through __index chains (instance -> Leaf -> Mid -> Base)
local Base = {}; Base.__index = Base
Base.scale = 2
function Base.new(x) return setmetatable({ x = x, n = 0 }, Base) end
function Base:inc(d) self.x = self.x + d; self.n = self.n + 1 end
function Base:get() return self.x * self.scale end
local Mid = setmetatable({}, { __index = Base }); Mid.__index = Mid
Mid.bias = 3
function Mid.new(x) local o = Base.new(x); o.w = 3; return setmetatable(o, Mid) end
function Mid:twice(d) self:inc(d); self:inc(d) end
local Leaf = setmetatable({}, { __index = Mid }); Leaf.__index = Leaf
function Leaf.new(x) local o = Mid.new(x); o.z = 1; return setmetatable(o, Leaf) end
function Leaf:total() return self:get() + self.bias + self.w end
local n = tonumber(arg and arg[1]) or 3000000
local o = Leaf.new(1)
local acc = 0
for i = 1, n do
  o:twice(i % 3)
  acc = (acc + o:total() + o.z) % 1000003
end
print(acc, o.x, o.n)
