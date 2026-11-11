-- J5 F1 informational program `field_rw` (docs/JIT_TRACE.md 10.2, task F1): read-modify-write of the fields of one
-- object (hash part, existing keys, ints and floats) in a numeric for loop. N = reps (arg[1]); inner n = 1000.
local function run(o, n)
  for i = 1, n do
    o.x = o.x + 1
    o.y = o.y * 0.999 + o.z
    o.n = o.n + o.x % 3
  end
end
local o = { x = 1, y = 0.5, n = 0, z = 3 }
local reps = tonumber(arg and arg[1]) or 150000
for r = 1, reps do run(o, 1000) end
print(o.x, string.format("%.6f", o.y), o.n)
