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
T(io.type)
T(io.type, nil)
T(io.type, 'abc', nil)
T(io.type, {1,2,3}, nil)
T(io.type, 5, nil, nil)
T(io.type, true)
T(io.type, 'abc', true)
T(io.type, {1,2,3}, true)
T(io.type, 5, true, true)
T(io.type, {})
T(io.type, 'abc', {})
T(io.type, {1,2,3}, {})
T(io.type, 5, {}, {})
T(io.type, 'abc')
