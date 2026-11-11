-- Same computation as sieve.lua written with `while` loops: fewer simultaneously live registers, so the
-- whole-function register allocator of the V2 variant (no spilling) fits. Used for the V2 data point only.
local function f(flags, n, reps)
  local total, r = 0, 0
  while r < reps do
    r = r + 1
    local i = 2
    while i <= n do
      flags[i] = true
      i = i + 1
    end
    i = 2
    while i <= n do
      local on = flags[i]
      if on then
        total = total + 1
        local j = i * i
        while j <= n do
          flags[j] = false
          j = j + i
        end
      end
      i = i + 1
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
