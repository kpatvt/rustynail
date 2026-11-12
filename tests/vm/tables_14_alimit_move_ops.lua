-- random table.insert/remove/move sequences with # and holes: the border after each step depends on alimit
local x = 12345
local function rnd(n) x = (x * 1103515245 + 12345) % 2147483648; return (x >> 7) % n end
local out, h = {}, 0
local function mix(v) h = (h * 31 + v) % 1000000007 end
for trial = 1, 400 do
  local n = rnd(40)
  local t = {}
  for i = 1, n do t[i] = i end
  if rnd(3) == 0 then t[rnd(n + 1) + 1] = nil end
  local u = {}
  for i = 1, rnd(30) do u[i] = -i end
  for step = 1, 40 do
    local op = rnd(9)
    local len = #t
    mix(len)
    if op == 0 then table.insert(t, step)
    elseif op == 1 then table.insert(t, rnd(len + 1) + 1, step)
    elseif op == 2 and len > 0 then mix(table.remove(t) or 0)
    elseif op == 3 and len > 0 then mix(table.remove(t, rnd(len) + 1) or 0)
    elseif op == 4 then table.move(t, 1 + rnd(5), 1 + rnd(len + 1), 1 + rnd(8))
    elseif op == 5 then table.move(t, 1 + rnd(3), rnd(len + 1), 1 + rnd(6), u)
    elseif op == 6 then t[rnd(len + 2) + 1] = nil
    elseif op == 7 then t[len + 1] = step
    else mix(#u) end
  end
  mix(#t); mix(#u)
  for k, v in pairs(t) do mix(k); mix(v) end
end
print(h)
