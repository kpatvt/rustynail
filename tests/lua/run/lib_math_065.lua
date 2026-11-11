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
T(math.tointeger)
T(math.tointeger, nil)
T(math.tointeger, 'abc', nil)
T(math.tointeger, {1,2,3}, nil)
T(math.tointeger, 5, nil, nil)
T(math.tointeger, true)
T(math.tointeger, 'abc', true)
T(math.tointeger, {1,2,3}, true)
T(math.tointeger, 5, true, true)
T(math.tointeger, {})
T(math.tointeger, 'abc', {})
T(math.tointeger, {1,2,3}, {})
T(math.tointeger, 5, {}, {})
T(math.tointeger, 'abc')
