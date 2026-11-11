local function show(...)
  local t = table.pack(...)
  for i = 1, t.n do
    local v = t[i]
    if type(v) == 'number' and math.type(v) == 'float' then t[i] = string.format('%.14g', v) .. 'f'
    elseif type(v) == 'table' or type(v) == 'function' or type(v) == 'thread' or type(v) == 'userdata' then t[i] = type(v)
    else t[i] = tostring(v) end
  end
  print(table.concat(t, ' | ', 1, t.n))
end
local function T(f, ...) show(pcall(f, ...)) end
T(math.max, 5, (1<<62), (1<<62))
T(math.max, '0x10')
T(math.max, 'abc', '0x10')
T(math.max, {1,2,3}, '0x10')
T(math.max, 5, '0x10', '0x10')
T(math.max, 1e300)
T(math.max, 'abc', 1e300)
T(math.max, {1,2,3}, 1e300)
T(math.max, 5, 1e300, 1e300)
T(math.max, -(2^63))
T(math.max, 'abc', -(2^63))
T(math.max, {1,2,3}, -(2^63))
T(math.max, 5, -(2^63), -(2^63))
T(math.min)
