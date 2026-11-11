-- (c) dead slot after a trace exit: debug.getlocal sees a stale value
local t = 0
for i = 1, 3000 do
  local d = i * 3
  t = t + d
  if i == 2500 then
    local s = {}
    for k = 1, 10 do
      local name, v = debug.getlocal(1, k)
      if not name then break end
      s[#s + 1] = name .. "=" .. tostring(v)
    end
    print(table.concat(s, " "), "expected d=" .. (i * 3))
  end
end
print(t)
