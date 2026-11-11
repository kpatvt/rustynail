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
T(io.close, nil)
T(io.close, 'abc', nil)
T(io.close, {1,2,3}, nil)
T(io.close, 5, nil, nil)
T(io.close, true)
T(io.close, 'abc', true)
T(io.close, {1,2,3}, true)
T(io.close, 5, true, true)
T(io.close, {})
T(io.close, 'abc', {})
T(io.close, {1,2,3}, {})
T(io.close, 5, {}, {})
T(io.close, 'abc')
T(io.close, 'abc', 'abc')
