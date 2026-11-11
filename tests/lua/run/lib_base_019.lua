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
T(rawget, 'abc', true)
T(rawget, {1,2,3}, true)
T(rawget, 5, true, true)
T(rawget, {})
T(rawget, 'abc', {})
T(rawget, {1,2,3}, {})
T(rawget, 5, {}, {})
T(rawget, 'abc')
T(rawget, 'abc', 'abc')
T(rawget, {1,2,3}, 'abc')
T(rawget, 5, 'abc', 'abc')
T(rawget, '10')
T(rawget, 'abc', '10')
T(rawget, {1,2,3}, '10')
