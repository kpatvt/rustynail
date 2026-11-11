-- short inner loops inside an outer loop: entry and exit of the inner trace thousands of times, trip counts 0..12
local function f(n)
  local s = 0
  for i = 1, n do
    for j = 1, i % 13 do s = s + j * i end
  end
  return s
end
for rep = 1, 5 do print(f(1500 + rep)) end
