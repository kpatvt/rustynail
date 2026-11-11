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
T(table.unpack, nil)
T(table.unpack, 'abc', nil)
T(table.unpack, {1,2,3}, nil)
T(table.unpack, 5, nil, nil)
T(table.unpack, true)
T(table.unpack, 'abc', true)
T(table.unpack, {1,2,3}, true)
T(table.unpack, 5, true, true)
T(table.unpack, {})
T(table.unpack, 'abc', {})
T(table.unpack, {1,2,3}, {})
T(table.unpack, 5, {}, {})
T(table.unpack, 'abc')
T(table.unpack, 'abc', 'abc')
