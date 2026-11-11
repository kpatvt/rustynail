-- Sieve: sieve of Eratosthenes over 5000 flags (port from the public are-we-fast-yet "Sieve" description).
-- Verification: 669 primes per iteration.  Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
local N = tonumber(arg and arg[1]) or 2000

local function sieve(flags, size)
  local primeCount = 0
  for i = 2, size do
    if flags[i - 1] then
      primeCount = primeCount + 1
      local k = i + i
      while k <= size do
        flags[k - 1] = false
        k = k + i
      end
    end
  end
  return primeCount
end

local total, ok = 0, 0
local flags = {}
for it = 1, N do
  for i = 1, 5000 do flags[i] = true end
  local c = sieve(flags, 5000)
  total = total + c
  if c == 669 then ok = ok + 1 end
end
print("sieve", ok, total)
