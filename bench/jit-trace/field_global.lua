-- J5 F1 informational program `field_global` (task F1): global variables (the _ENV upvalue) read and written in a
-- numeric for loop, and a global function called from it (inlined). N = reps (arg[1]); inner n = 1000.
G_COUNT = 0
G_SUM = 0.0
function G_ADD(x) G_COUNT = G_COUNT + x end
local function run(n)
  for i = 1, n do
    G_ADD(i)
    G_SUM = G_SUM + i * 0.5
  end
end
local reps = tonumber(arg and arg[1]) or 150000
for r = 1, reps do run(1000) end
print(G_COUNT, G_SUM)
