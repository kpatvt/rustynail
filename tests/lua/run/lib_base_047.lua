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
T(next, 5, true, true)
T(next, {})
T(next, 'abc', {})
T(next, {1,2,3}, {})
T(next, 5, {}, {})
T(next, 'abc')
T(next, 'abc', 'abc')
T(next, {1,2,3}, 'abc')
T(next, 5, 'abc', 'abc')
T(next, '10')
T(next, 'abc', '10')
T(next, {1,2,3}, '10')
T(next, 5, '10', '10')
T(next, 1.5)
