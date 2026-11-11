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
T(math.ult, 'abc', true)
T(math.ult, {1,2,3}, true)
T(math.ult, 5, true, true)
T(math.ult, {})
T(math.ult, 'abc', {})
T(math.ult, {1,2,3}, {})
T(math.ult, 5, {}, {})
T(math.ult, 'abc')
T(math.ult, 'abc', 'abc')
T(math.ult, {1,2,3}, 'abc')
T(math.ult, 5, 'abc', 'abc')
T(math.ult, '10')
T(math.ult, 'abc', '10')
T(math.ult, {1,2,3}, '10')
