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
T(load, nil)
T(load, 'abc', nil)
T(load, {1,2,3}, nil)
T(load, 5, nil, nil)
T(load, true)
T(load, 'abc', true)
T(load, {1,2,3}, true)
T(load, 5, true, true)
T(load, {})
T(load, 'abc', {})
T(load, {1,2,3}, {})
T(load, 5, {}, {})
T(load, 'abc')
T(load, 'abc', 'abc')
