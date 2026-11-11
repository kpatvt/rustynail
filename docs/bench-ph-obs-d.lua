-- (d) sunk allocations are not charged to the collector: pacing and finalizer timing differ
collectgarbage("incremental")
local fin = 0
for i = 1, 200 do setmetatable({}, {__gc = function() fin = fin + 1 end}) end
local x = 0
for i = 1, 3000000 do
  local t = {a = i, b = i}
  x = x + t.a + t.b
end
print("finalizers run during loop:", fin, "count KB < 1000:", collectgarbage("count") < 1000)
