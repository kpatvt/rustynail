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
T(math.ceil, true)
T(math.ceil, 'abc', true)
T(math.ceil, {1,2,3}, true)
T(math.ceil, 5, true, true)
T(math.ceil, {})
T(math.ceil, 'abc', {})
T(math.ceil, {1,2,3}, {})
T(math.ceil, 5, {}, {})
T(math.ceil, 'abc')
T(math.ceil, 'abc', 'abc')
T(math.ceil, {1,2,3}, 'abc')
T(math.ceil, 5, 'abc', 'abc')
T(math.ceil, '10')
T(math.ceil, 'abc', '10')
