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
T(string.char, {1,2,3}, -(2^63))
T(string.char, 5, -(2^63), -(2^63))
T(string.format)
T(string.format, nil)
T(string.format, 'abc', nil)
T(string.format, {1,2,3}, nil)
T(string.format, 5, nil, nil)
T(string.format, true)
T(string.format, 'abc', true)
T(string.format, {1,2,3}, true)
T(string.format, 5, true, true)
T(string.format, {})
T(string.format, 'abc', {})
T(string.format, {1,2,3}, {})
