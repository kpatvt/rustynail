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
T(type, 5, '0x10', '0x10')
T(type, 1e300)
T(type, 'abc', 1e300)
T(type, {1,2,3}, 1e300)
T(type, 5, 1e300, 1e300)
T(type, -(2^63))
T(type, 'abc', -(2^63))
T(type, {1,2,3}, -(2^63))
T(type, 5, -(2^63), -(2^63))
T(tostring)
T(tostring, nil)
T(tostring, 'abc', nil)
T(tostring, {1,2,3}, nil)
T(tostring, 5, nil, nil)
