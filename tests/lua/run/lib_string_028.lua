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
T(string.byte, 'abc', nil)
T(string.byte, {1,2,3}, nil)
T(string.byte, 5, nil, nil)
T(string.byte, true)
T(string.byte, 'abc', true)
T(string.byte, {1,2,3}, true)
T(string.byte, 5, true, true)
T(string.byte, {})
T(string.byte, 'abc', {})
T(string.byte, {1,2,3}, {})
T(string.byte, 5, {}, {})
T(string.byte, 'abc')
T(string.byte, 'abc', 'abc')
T(string.byte, {1,2,3}, 'abc')
