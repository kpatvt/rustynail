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
T(select, nil)
T(select, 'abc', nil)
T(select, {1,2,3}, nil)
T(select, 5, nil, nil)
T(select, true)
T(select, 'abc', true)
T(select, {1,2,3}, true)
T(select, 5, true, true)
T(select, {})
T(select, 'abc', {})
T(select, {1,2,3}, {})
T(select, 5, {}, {})
T(select, 'abc')
T(select, 'abc', 'abc')
