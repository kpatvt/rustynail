local n = tonumber(arg and arg[1]) or 1500
local keep = {}
local total = 0
for r = 1, n do
  local batch = {}
  for i = 1, 2000 do batch[i] = {i, tostring(i), {r}} ; batch[i][4] = function() return i end end
  keep[r % 20 + 1] = batch
  total = total + #batch + batch[2000][3][1] + batch[7][4]()
end
local weak = setmetatable({}, {__mode = "k"})
for i = 1, 100000 do weak[{}] = i end
collectgarbage()
print(total, collectgarbage("count") > 0)
