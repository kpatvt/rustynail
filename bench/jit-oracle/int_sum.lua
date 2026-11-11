-- JIT oracle program 1: integer multiply/shift/add/sub chain in a numeric for loop.
local function f(N)
  local s = 0
  for i = 1, N do s = s + i * 3 - (i >> 1) end
  return s
end
print(f(tonumber(arg and arg[1]) or 200000000))
