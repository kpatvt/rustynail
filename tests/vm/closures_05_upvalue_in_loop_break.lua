local fs = {}
for i = 1, 10 do
  local x = i
  fs[#fs + 1] = function() return x end
  if i == 4 then break end
end
print(#fs, fs[1](), fs[4]())
local gs = {}
for i = 1, 3 do
  for j = 1, 3 do
    gs[#gs + 1] = function() return i * 10 + j end
  end
end
print(#gs, gs[1](), gs[5](), gs[9]())
local k = 0
local hs = {}
while true do
  k = k + 1
  local c = k
  hs[k] = function() return c end
  if k == 3 then goto done end
end
::done::
print(hs[1](), hs[2](), hs[3]())
