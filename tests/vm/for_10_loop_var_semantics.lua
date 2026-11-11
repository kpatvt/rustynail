for i = 1, 3 do
  local j = i
  j = j * 2
  io.write(j, " ")
end
print()
print(load("for i = 1, 3 do i = 1 end"))
print(load("for i in pairs({}) do i = 1 end"))
local x = 0
for i = 1, 3 do x = x + i end
print(x)
