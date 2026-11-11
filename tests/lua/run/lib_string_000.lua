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
T(string.len)
T(string.len, nil)
T(string.len, 'abc', nil)
T(string.len, {1,2,3}, nil)
T(string.len, 5, nil, nil)
T(string.len, true)
T(string.len, 'abc', true)
T(string.len, {1,2,3}, true)
T(string.len, 5, true, true)
T(string.len, {})
T(string.len, 'abc', {})
T(string.len, {1,2,3}, {})
T(string.len, 5, {}, {})
T(string.len, 'abc')
