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
T(rawequal, 'abc', nil)
T(rawequal, {1,2,3}, nil)
T(rawequal, 5, nil, nil)
T(rawequal, true)
T(rawequal, 'abc', true)
T(rawequal, {1,2,3}, true)
T(rawequal, 5, true, true)
T(rawequal, {})
T(rawequal, 'abc', {})
T(rawequal, {1,2,3}, {})
T(rawequal, 5, {}, {})
T(rawequal, 'abc')
T(rawequal, 'abc', 'abc')
T(rawequal, {1,2,3}, 'abc')
