-- JIT oracle program 2: two float multiply-add recurrences (latency bound) in a for loop.
local function f(N)
  local x, acc = 0.5, 0.0
  for i = 1, N do
    acc = acc * 0.999 + x * 1.5 - 0.25
    x = x * 0.75 + 0.1
  end
  return acc
end
print(f(tonumber(arg and arg[1]) or 100000000))
