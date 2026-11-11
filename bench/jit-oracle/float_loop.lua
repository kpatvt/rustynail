-- JIT oracle program 7: float for loop (N is ignored: the limit is a constant).
local function f(N)
  local s = 0.0
  for x = 0.5, 150000000.0, 0.5 do s = s + x * 0.25 end
  return s
end
print(f(tonumber(arg and arg[1]) or 0))
