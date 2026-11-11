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
T(io.read, 'abc', true)
T(io.read, {1,2,3}, true)
T(io.read, 5, true, true)
T(io.read, {})
T(io.read, 'abc', {})
T(io.read, {1,2,3}, {})
T(io.read, 5, {}, {})
T(io.read, 'abc')
T(io.read, 'abc', 'abc')
T(io.read, {1,2,3}, 'abc')
T(io.read, 5, 'abc', 'abc')
T(io.read, '10')
T(io.read, 'abc', '10')
T(io.read, {1,2,3}, '10')
