-- docs/JIT_TRACE.md section 78.4 (`outerlink`): the exits at the end of an inner loop link through the untraced outer loop's head into the inner
-- loop's trace; outer bodies with work, a type change of the inner step, an error and a collection.
local K = 64
local S, ring, acc = {}, {}, 0
for k = 1, K do S[k] = "w" .. k end
for o = 0, 300 do
  for k = 1, K do
    local r = S[k]
    ring[k] = r
    acc = acc + #r
  end
  if o % 50 == 0 then acc = acc + o end
end
local m = 0
for y = 1, 200 do
  local ci = y / 100 - 1
  for x = 1, 40 do m = m + (x * ci) % 3 end
  if y == 150 then collectgarbage() end
end
local ok = pcall(function()
  for o = 1, 200 do
    for k = 1, K do acc = acc + k end
    if o == 180 then error("x") end
  end
end)
local f = 0
for o = 1, 100 do
  local step = (o > 60) and 0.5 or 1
  for k = 1, 30 do f = f + step end
end
print(acc, ring[K], m, ok, f)
