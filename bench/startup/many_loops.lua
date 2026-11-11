-- compile-cost stress: 300 distinct functions, each with its own small hot loop (3000 iterations): every one of them
-- is recorded and compiled once by a tracing JIT; the run is dominated by record + compile time, not by loop time.
local total = 0
for f = 1, 300 do
  local src = ("local n = ... local s = 0 for i = 1, n do s = s + i * %d %% 7 + (i & %d) end return s"):format(f, f)
  local fn = assert(load(src, "=f" .. f))
  total = total + fn(3000)
end
print(total)
