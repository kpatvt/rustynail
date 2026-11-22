-- docs/JIT_TRACE.md section 38 (`deepprobe`): three deep sites with known probe counts (tr_core::dp_deep_probe_programs pins the number of
-- dropped recordings in `always` mode).

local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end

-- (1) the deep iteration comes every fourth time: the probes see it (their strides alternate between two and three iterations), so the tries
-- after them are dropped until a deep iteration comes round again
local s = 0
for i = 1, 3000 do
  if i % 4 == 1 then s = s + fib(7) else s = s + fib(3) end
end
print("tries", s)

-- (2) every iteration has the same call tree: the site stops probing after four probes with one path
local u = 0
for i = 1, 3000 do u = u + fib(6) + i % 2 end
print("uniform", u)

-- (3) iterations that run an inner loop abort their recording (the site stops probing at the first abort and compiles the next recording that
-- closes)
local w = 0
for i = 1, 6000 do
  if i % 2 == 0 then
    for j = 1, 3 do w = w + j end
  end
  w = w + fib(6)
end
print("abort", w)
