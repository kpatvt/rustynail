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
T(table.insert, 5, '0x10', '0x10')
T(table.insert, 1e300)
T(table.insert, 'abc', 1e300)
T(table.insert, {1,2,3}, 1e300)
T(table.insert, 5, 1e300, 1e300)
T(table.insert, -(2^63))
T(table.insert, 'abc', -(2^63))
T(table.insert, {1,2,3}, -(2^63))
T(table.insert, 5, -(2^63), -(2^63))
T(table.remove)
T(table.remove, nil)
T(table.remove, 'abc', nil)
T(table.remove, {1,2,3}, nil)
T(table.remove, 5, nil, nil)
