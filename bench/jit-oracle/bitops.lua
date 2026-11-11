-- JIT oracle program 5: xorshift64 generator, shifts/xor/and (integer).
local function f(N)
  local x, acc = 88172645463325252, 0
  for i = 1, N do
    x = x ~ (x << 13)
    x = x ~ (x >> 7)
    x = x ~ (x << 17)
    acc = acc + (x & 0xFFFF)
  end
  return acc
end
print(f(tonumber(arg and arg[1]) or 100000000))
