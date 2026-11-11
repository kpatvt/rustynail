local fs = {}
do
  local i = 1
  ::again::
  local x = i * 10
  fs[#fs + 1] = function() x = x + 1 return x end
  i = i + 1
  if i <= 3 then goto again end
end
print(fs[1](), fs[1](), fs[2](), fs[3]())
do
  local k = 0
  ::l1::
  k = k + 1
  if k < 5 then goto l1 end
  print(k)
end
do
  goto fwd
  local z
  ::fwd::
end
print("ok")
