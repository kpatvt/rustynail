local fs = {}
for i = 1, 60 do
  local x = i * 2
  fs[#fs + 1] = function() x = x + 1; return x end
end
local s = 0
for r = 1, 3 do for _, f in ipairs(fs) do s = s + f() end end
print(s)

local t = {}
for i = 1, 50 do t["k" .. i] = i end
local sum, cnt = 0, 0
for r = 1, 4 do
  for k, v in pairs(t) do sum = sum + v; cnt = cnt + 1 end
  for i, v in ipairs({1, 2, 3, 4, 5}) do sum = sum + i * v end
end
print(sum, cnt)

local i, acc = 0, 0
::top::
i = i + 1
if i % 3 == 0 then acc = acc + i end
if i < 100 then goto top end
print(acc)

local n, c2 = 0, 0
repeat
  local y = n * 2
  c2 = c2 + y
  n = n + 1
until n > 50 or c2 > 1000
print(n, c2)
