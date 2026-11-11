-- upvalue accumulators written by inlined closures; the callee changes between calls of the loop
local function mk(step)
  local acc = 0
  return function(x) acc = acc + step * x return acc end, function() return acc end
end
local inc, get = mk(2)
local inc3 = mk(3)
local function run(n, f)
  local s = 0
  for i = 1, n do s = s + f(i) % 5 end
  return s
end
for rep = 1, 6 do
  print(run(800, inc), get())
  print(run(800, inc3))
end
