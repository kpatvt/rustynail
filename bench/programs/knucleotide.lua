local n = tonumber(arg and arg[1]) or 5000000
local last = 42
local function rnd() last = (last * 3877 + 29573) % 139968; return last end
local t = {}
local bases = {"a", "c", "g", "t"}
for i = 1, n do t[i] = bases[rnd() % 4 + 1] end
local seq = table.concat(t)
local result = {}
for frame = 1, 4 do
  local counts = {}
  for i = 1, #seq - frame + 1 do local k = seq:sub(i, i + frame - 1); counts[k] = (counts[k] or 0) + 1 end
  local keys = {}
  for k in pairs(counts) do keys[#keys + 1] = k end
  table.sort(keys, function(a, b) return counts[a] > counts[b] or (counts[a] == counts[b] and a < b) end)
  result[#result + 1] = keys[1] .. "=" .. counts[keys[1]]
end
print(table.concat(result, " "))
