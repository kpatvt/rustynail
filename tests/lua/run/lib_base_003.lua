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
T(type, 'abc', function() end)
T(type, {1,2,3}, function() end)
T(type, 5, function() end, function() end)
T(type, io.stdout)
T(type, 'abc', io.stdout)
T(type, {1,2,3}, io.stdout)
T(type, 5, io.stdout, io.stdout)
T(type, (1<<62))
T(type, 'abc', (1<<62))
T(type, {1,2,3}, (1<<62))
T(type, 5, (1<<62), (1<<62))
T(type, '0x10')
T(type, 'abc', '0x10')
T(type, {1,2,3}, '0x10')
