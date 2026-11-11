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
T(type)
T(type, nil)
T(type, 'abc', nil)
T(type, {1,2,3}, nil)
T(type, 5, nil, nil)
T(type, true)
T(type, 'abc', true)
T(type, {1,2,3}, true)
T(type, 5, true, true)
T(type, {})
T(type, 'abc', {})
T(type, {1,2,3}, {})
T(type, 5, {}, {})
T(type, 'abc')
