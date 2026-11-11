local floor = math.floor
local unpack = unpack or table.unpack
local n = tonumber(arg and arg[1]) or 16000000
local function sum(...) local s = 0; for i = 1, select('#', ...) do s = s + (select(i, ...)) end; return s end
local function pass(...) return ... end
local function pack2(...) local t = {n = select('#', ...), ...}; return t.n end
local t = {1, 2, 3, 4, 5, 6, 7, 8}
local s = 0
for i = 1, floor(n / 4) do
  s = s + sum(i, 2, 3, 4) + select('#', pass(i, i, i)) + pack2(i, i) + select(2, unpack(t, 1, 5))
end
print(string.format("%d", s))
