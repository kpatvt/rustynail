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
T(tostring, true)
T(tostring, 'abc', true)
T(tostring, {1,2,3}, true)
T(tostring, 5, true, true)
T(tostring, {})
T(tostring, 'abc', {})
T(tostring, {1,2,3}, {})
T(tostring, 5, {}, {})
T(tostring, 'abc')
T(tostring, 'abc', 'abc')
T(tostring, {1,2,3}, 'abc')
T(tostring, 5, 'abc', 'abc')
T(tostring, '10')
T(tostring, 'abc', '10')
