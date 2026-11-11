-- table.move / table.unpack / table.concat / table.sort on 1000-element arrays
local move, unpack, concat, sort = table.move, table.unpack, table.concat, table.sort
local t = {}
for j = 1, 1000 do t[j] = (j * 7919) % 1009 end
local u = {}
local acc = 0
for i = 1, 10000 do
  move(t, 1, 1000, 1, u)
  acc = acc + select("#", unpack(u, 1, 200)) + #concat(u, ",", 1, 100)
end
for i = 1, 100 do
  move(t, 1, 1000, 1, u)
  sort(u)
  acc = acc + u[1]
end
print(acc)
