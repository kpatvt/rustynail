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
T(table.pack, 'abc', true)
T(table.pack, {1,2,3}, true)
T(table.pack, 5, true, true)
T(table.pack, {})
T(table.pack, 'abc', {})
T(table.pack, {1,2,3}, {})
T(table.pack, 5, {}, {})
T(table.pack, 'abc')
T(table.pack, 'abc', 'abc')
T(table.pack, {1,2,3}, 'abc')
T(table.pack, 5, 'abc', 'abc')
T(table.pack, '10')
T(table.pack, 'abc', '10')
T(table.pack, {1,2,3}, '10')
