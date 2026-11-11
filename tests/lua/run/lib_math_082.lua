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
T(math.min, {1,2,3}, function() end)
T(math.min, 5, function() end, function() end)
T(math.min, io.stdout)
T(math.min, 'abc', io.stdout)
T(math.min, {1,2,3}, io.stdout)
T(math.min, 5, io.stdout, io.stdout)
T(math.min, (1<<62))
T(math.min, 'abc', (1<<62))
T(math.min, {1,2,3}, (1<<62))
T(math.min, 5, (1<<62), (1<<62))
T(math.min, '0x10')
T(math.min, 'abc', '0x10')
T(math.min, {1,2,3}, '0x10')
T(math.min, 5, '0x10', '0x10')
