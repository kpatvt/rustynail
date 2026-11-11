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
T(math.tan, 'abc', nil)
T(math.tan, {1,2,3}, nil)
T(math.tan, 5, nil, nil)
T(math.tan, true)
T(math.tan, 'abc', true)
T(math.tan, {1,2,3}, true)
T(math.tan, 5, true, true)
T(math.tan, {})
T(math.tan, 'abc', {})
T(math.tan, {1,2,3}, {})
T(math.tan, 5, {}, {})
T(math.tan, 'abc')
T(math.tan, 'abc', 'abc')
T(math.tan, {1,2,3}, 'abc')
