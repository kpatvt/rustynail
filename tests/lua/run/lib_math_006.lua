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
T(math.ceil, {1,2,3}, '10')
T(math.ceil, 5, '10', '10')
T(math.ceil, 1.5)
T(math.ceil, 'abc', 1.5)
T(math.ceil, {1,2,3}, 1.5)
T(math.ceil, 5, 1.5, 1.5)
T(math.ceil, 2^63)
T(math.ceil, 'abc', 2^63)
T(math.ceil, {1,2,3}, 2^63)
T(math.ceil, 5, 2^63, 2^63)
T(math.ceil, -1)
T(math.ceil, 'abc', -1)
T(math.ceil, {1,2,3}, -1)
T(math.ceil, 5, -1, -1)
