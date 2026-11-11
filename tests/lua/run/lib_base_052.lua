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
T(pairs, 'abc')
T(pairs, 'abc', 'abc')
T(pairs, {1,2,3}, 'abc')
T(pairs, 5, 'abc', 'abc')
T(pairs, '10')
T(pairs, 'abc', '10')
T(pairs, {1,2,3}, '10')
T(pairs, 5, '10', '10')
T(pairs, 1.5)
T(pairs, 'abc', 1.5)
T(pairs, {1,2,3}, 1.5)
T(pairs, 5, 1.5, 1.5)
T(pairs, 2^63)
T(pairs, 'abc', 2^63)
