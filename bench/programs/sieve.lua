local N = tonumber(arg and arg[1]) or 12000000
local total = 0
for r = 1, 3 do
  local flags = {}
  for i = 2, N do flags[i] = true end
  for i = 2, math.floor(math.sqrt(N)) do
    if flags[i] then for j = i * i, N, i do flags[j] = false end end
  end
  local c = 0
  for i = 2, N do if flags[i] then c = c + 1 end end
  total = total + c
end
print(total)
