-- Table bucket: array writes and reads (sieve of Eratosthenes, repeated).
-- File protocol (see crates/rn-jit/examples/oracle/staterun.rs): f = hot function, setup = builds the tables.
local function f(flags, n, reps)
  local total = 0
  for r = 1, reps do
    for i = 2, n do flags[i] = true end
    for i = 2, n do
      if flags[i] then
        total = total + 1
        for j = i * i, n, i do flags[j] = false end
      end
    end
  end
  return total
end

local function setup(reps)
  local n = 100000
  local flags = {}
  for i = 1, n do flags[i] = true end
  return flags, n, reps
end

if ORACLE then return f, setup end
print(f(setup(tonumber(arg and arg[1]) or 200)))
