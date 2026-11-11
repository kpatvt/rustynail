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
T(string.unpack)
T(string.unpack, nil)
T(string.unpack, 'abc', nil)
T(string.unpack, {1,2,3}, nil)
T(string.unpack, 5, nil, nil)
T(string.unpack, true)
T(string.unpack, 'abc', true)
T(string.unpack, {1,2,3}, true)
T(string.unpack, 5, true, true)
T(string.unpack, {})
T(string.unpack, 'abc', {})
T(string.unpack, {1,2,3}, {})
T(string.unpack, 5, {}, {})
T(string.unpack, 'abc')
