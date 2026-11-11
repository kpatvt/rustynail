-- JIT oracle program 4: collatz step counts with %, //, comparisons (integer while loop).
local function f(N)
  local maxlen, total = 0, 0
  for n = 1, N do
    local x, len = n, 0
    while x ~= 1 do
      if x % 2 == 0 then x = x // 2 else x = 3 * x + 1 end
      len = len + 1
    end
    total = total + len
    if len > maxlen then maxlen = len end
  end
  return total * 1000 + maxlen
end
print(f(tonumber(arg and arg[1]) or 700000))
