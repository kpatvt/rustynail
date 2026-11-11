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
T(io.open, true)
T(io.open, 'abc', true)
T(io.open, {1,2,3}, true)
T(io.open, 5, true, true)
T(io.open, {})
T(io.open, 'abc', {})
T(io.open, {1,2,3}, {})
T(io.open, 5, {}, {})
T(io.open, 'abc')
T(io.open, 'abc', 'abc')
T(io.open, {1,2,3}, 'abc')
T(io.open, 5, 'abc', 'abc')
T(io.open, '10')
T(io.open, 'abc', '10')
