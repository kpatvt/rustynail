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
T(table.pack, 'abc', -(2^63))
T(table.pack, {1,2,3}, -(2^63))
T(table.pack, 5, -(2^63), -(2^63))
T(table.move)
T(table.move, nil)
T(table.move, 'abc', nil)
T(table.move, {1,2,3}, nil)
T(table.move, 5, nil, nil)
T(table.move, true)
T(table.move, 'abc', true)
T(table.move, {1,2,3}, true)
T(table.move, 5, true, true)
T(table.move, {})
T(table.move, 'abc', {})
