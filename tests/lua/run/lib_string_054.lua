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
T(string.gmatch, function() end)
T(string.gmatch, 'abc', function() end)
T(string.gmatch, {1,2,3}, function() end)
T(string.gmatch, 5, function() end, function() end)
T(string.gmatch, io.stdout)
T(string.gmatch, 'abc', io.stdout)
T(string.gmatch, {1,2,3}, io.stdout)
T(string.gmatch, 5, io.stdout, io.stdout)
T(string.gmatch, (1<<62))
T(string.gmatch, 'abc', (1<<62))
T(string.gmatch, {1,2,3}, (1<<62))
T(string.gmatch, 5, (1<<62), (1<<62))
T(string.gmatch, '0x10')
T(string.gmatch, 'abc', '0x10')
