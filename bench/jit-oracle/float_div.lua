-- JIT oracle program 6: float division and addition mix.
local function f(N)
  local s, x = 0.0, 1.0
  for i = 1, N do
    s = s + x / (x + 1.5)
    x = x + 0.5
  end
  return s
end
print(f(tonumber(arg and arg[1]) or 150000000))
