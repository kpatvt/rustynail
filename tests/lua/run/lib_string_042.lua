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
T(string.find, {1,2,3}, nil)
T(string.find, 5, nil, nil)
T(string.find, true)
T(string.find, 'abc', true)
T(string.find, {1,2,3}, true)
T(string.find, 5, true, true)
T(string.find, {})
T(string.find, 'abc', {})
T(string.find, {1,2,3}, {})
T(string.find, 5, {}, {})
T(string.find, 'abc')
T(string.find, 'abc', 'abc')
T(string.find, {1,2,3}, 'abc')
T(string.find, 5, 'abc', 'abc')
