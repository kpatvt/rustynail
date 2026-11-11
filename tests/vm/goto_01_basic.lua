do
  local i = 1
  ::top::
  if i <= 3 then io.write(i, " ") i = i + 1 goto top end
  print()
end
for i = 1, 3 do
  for j = 1, 3 do
    if j == 2 then goto continue end
    io.write(i, ":", j, " ")
    ::continue::
  end
end
print()
goto skip
print("not printed")
::skip::
print("after skip")
