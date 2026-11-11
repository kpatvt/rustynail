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
T(rawlen, {1,2,3}, true)
T(rawlen, 5, true, true)
T(rawlen, {})
T(rawlen, 'abc', {})
T(rawlen, {1,2,3}, {})
T(rawlen, 5, {}, {})
T(rawlen, 'abc')
T(rawlen, 'abc', 'abc')
T(rawlen, {1,2,3}, 'abc')
T(rawlen, 5, 'abc', 'abc')
T(rawlen, '10')
T(rawlen, 'abc', '10')
T(rawlen, {1,2,3}, '10')
T(rawlen, 5, '10', '10')
