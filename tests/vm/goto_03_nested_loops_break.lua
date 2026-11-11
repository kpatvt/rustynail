local found
for i = 1, 5 do
  for j = 1, 5 do
    if i * j == 12 then found = {i, j} goto out end
  end
end
::out::
print(found[1], found[2])
local n = 0
while true do
  n = n + 1
  if n > 100 then break end
  if n % 2 == 0 then goto cont end
  if n > 10 then break end
  ::cont::
end
print(n)
repeat
  local x = 5
  if x then goto fin end
  print("no")
  ::fin::
until true
print("done")
