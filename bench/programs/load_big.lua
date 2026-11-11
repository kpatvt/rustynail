local n = tonumber(arg and arg[1]) or 3000
local parts = {}
for i = 1, n do
  parts[#parts + 1] = string.format("F[%d] = function(a, b) local t = {x = a, y = b, [%d] = 'v%d'} if a > b then return t.x * %d + #t else return t.y - %d end end", i, i, i, i, i)
  if i % 150 == 0 then parts[#parts + 1] = "do local s = 0 for i = 1, 3 do s = s + i end end" end
end
local src = "local F = {}\n" .. table.concat(parts, "\n")
local total = 0
for r = 1, 25 do
  local f = assert(load(src))
  total = total + #string.dump(f)
end
print(#src, total)
