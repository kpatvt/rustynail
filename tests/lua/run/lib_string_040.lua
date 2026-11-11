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
T(string.format, 5, print, print)
T(string.format, function() end)
T(string.format, 'abc', function() end)
T(string.format, {1,2,3}, function() end)
T(string.format, 5, function() end, function() end)
T(string.format, io.stdout)
T(string.format, 'abc', io.stdout)
T(string.format, {1,2,3}, io.stdout)
T(string.format, 5, io.stdout, io.stdout)
T(string.format, (1<<62))
T(string.format, 'abc', (1<<62))
T(string.format, {1,2,3}, (1<<62))
T(string.format, 5, (1<<62), (1<<62))
T(string.format, '0x10')
