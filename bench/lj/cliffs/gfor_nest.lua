-- N7: nested generic for: the CALL of ipairs(t) / pairs(t) inside a traced outer loop.
--   lua gfor_nest.lua <ipairs|pairs> <size> <work|empty>   (the outer count is scaled so every variant does about 3M inner steps)
local kind = arg and arg[1] or "ipairs"
local n = tonumber(arg and arg[2] or 100)
local work = (arg and arg[3] or "work") == "work"
local t = {}
for j = 1, n do t[j] = j end
local reps = math.floor(3000000 / n)
local acc = 0
local it = kind == "ipairs" and ipairs or pairs
if kind == "ipairs" then
  if work then
    for r = 1, reps do for i, v in ipairs(t) do acc = acc + v * 2 + i end end
  else
    for r = 1, reps do for i, v in ipairs(t) do end end
  end
else
  if work then
    for r = 1, reps do for k, v in pairs(t) do acc = acc + v * 2 + k end end
  else
    for r = 1, reps do for k, v in pairs(t) do end end
  end
end
print(acc)
