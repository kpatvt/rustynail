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
T(math.atan, {1,2,3}, nil)
T(math.atan, 5, nil, nil)
T(math.atan, true)
T(math.atan, 'abc', true)
T(math.atan, {1,2,3}, true)
T(math.atan, 5, true, true)
T(math.atan, {})
T(math.atan, 'abc', {})
T(math.atan, {1,2,3}, {})
T(math.atan, 5, {}, {})
T(math.atan, 'abc')
T(math.atan, 'abc', 'abc')
T(math.atan, {1,2,3}, 'abc')
T(math.atan, 5, 'abc', 'abc')
