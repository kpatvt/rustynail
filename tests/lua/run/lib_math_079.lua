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
T(math.min, nil)
T(math.min, 'abc', nil)
T(math.min, {1,2,3}, nil)
T(math.min, 5, nil, nil)
T(math.min, true)
T(math.min, 'abc', true)
T(math.min, {1,2,3}, true)
T(math.min, 5, true, true)
T(math.min, {})
T(math.min, 'abc', {})
T(math.min, {1,2,3}, {})
T(math.min, 5, {}, {})
T(math.min, 'abc')
T(math.min, 'abc', 'abc')
