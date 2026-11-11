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
T(table.sort, 'abc', nil)
T(table.sort, {1,2,3}, nil)
T(table.sort, 5, nil, nil)
T(table.sort, true)
T(table.sort, 'abc', true)
T(table.sort, {1,2,3}, true)
T(table.sort, 5, true, true)
T(table.sort, {})
T(table.sort, 'abc', {})
T(table.sort, {1,2,3}, {})
T(table.sort, 5, {}, {})
T(table.sort, 'abc')
T(table.sort, 'abc', 'abc')
T(table.sort, {1,2,3}, 'abc')
