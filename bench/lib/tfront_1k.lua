-- positional table.insert / table.remove at the front of a 1000-element array (queue-like use): 2 x 1000 element moves per iteration
local insert, remove = table.insert, table.remove
local t = {}
for j = 1, 1000 do t[j] = j end
local acc = 0
for i = 1, 120000 do
  insert(t, 1, i)
  acc = acc + remove(t, 1)
end
print(acc)
