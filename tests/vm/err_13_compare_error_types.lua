local objs = {1, "a", {}, print, true, false}
for i = 1, 6 do
  for j = 1, 6 do
    local ok, e = pcall(function() return objs[i] < objs[j] end)
    io.write(ok and "ok" or "err", " ")
  end
  print()
end
for i = 1, 6 do
  for j = 1, 6 do
    local ok, e = pcall(function() return objs[i] <= objs[j] end)
    if not ok then io.write(e, "|") end
  end
end
print()
print(pcall(function() return objs[3] < objs[3] end))
print(pcall(function() return objs[1] < objs[3] end))
print(pcall(function() return objs[3] <= objs[2] end))
