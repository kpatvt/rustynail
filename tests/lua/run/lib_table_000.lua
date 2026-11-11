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
T(table.insert)
T(table.insert, nil)
T(table.insert, 'abc', nil)
T(table.insert, {1,2,3}, nil)
T(table.insert, 5, nil, nil)
T(table.insert, true)
T(table.insert, 'abc', true)
T(table.insert, {1,2,3}, true)
T(table.insert, 5, true, true)
T(table.insert, {})
T(table.insert, 'abc', {})
T(table.insert, {1,2,3}, {})
T(table.insert, 5, {}, {})
T(table.insert, 'abc')
