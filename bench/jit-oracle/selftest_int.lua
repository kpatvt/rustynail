-- Oracle self-test (not a benchmark): negative operands for %, //, shifts, bit ops and negative-step loops.
local function f(N)
  local acc = 0
  for i = -N, N, 3 do
    acc = acc + i % 7 + i // 5 + i % -3 + i // -4
    acc = acc ~ (i << 3) ~ (i >> 2) + (-i)
    local s = i & 63
    acc = acc + (1 << s) + (i >> s) + (i << s)
    acc = acc ~ (i << (s - 70)) ~ (i >> (s - 70)) ~ (i >> (s + 30))
    local d = (s | 1) - 32
    acc = acc + i % d + i // d
  end
  for i = N, -N, -7 do acc = acc + i end
  for i = 5, 1 do acc = acc + 1000000 end
  return acc
end
print(f(tonumber(arg and arg[1]) or 1000))
