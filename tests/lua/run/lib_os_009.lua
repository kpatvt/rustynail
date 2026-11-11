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
T(os.difftime, -(2^63))
T(os.difftime, 'abc', -(2^63))
T(os.difftime, {1,2,3}, -(2^63))
T(os.difftime, 5, -(2^63), -(2^63))
T(os.setlocale)
T(os.setlocale, nil)
T(os.setlocale, 'abc', nil)
T(os.setlocale, {1,2,3}, nil)
T(os.setlocale, 5, nil, nil)
T(os.setlocale, true)
T(os.setlocale, 'abc', true)
T(os.setlocale, {1,2,3}, true)
T(os.setlocale, 5, true, true)
T(os.setlocale, {})
