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
T(utf8.char)
T(utf8.char, nil)
T(utf8.char, 'abc', nil)
T(utf8.char, {1,2,3}, nil)
T(utf8.char, 5, nil, nil)
T(utf8.char, true)
T(utf8.char, 'abc', true)
T(utf8.char, {1,2,3}, true)
T(utf8.char, 5, true, true)
T(utf8.char, {})
T(utf8.char, 'abc', {})
T(utf8.char, {1,2,3}, {})
T(utf8.char, 5, {}, {})
T(utf8.char, 'abc')
