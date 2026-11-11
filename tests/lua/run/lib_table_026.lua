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
T(table.move, {1,2,3}, print)
T(table.move, 5, print, print)
T(table.move, function() end)
T(table.move, 'abc', function() end)
T(table.move, {1,2,3}, function() end)
T(table.move, 5, function() end, function() end)
T(table.move, io.stdout)
T(table.move, 'abc', io.stdout)
T(table.move, {1,2,3}, io.stdout)
T(table.move, 5, io.stdout, io.stdout)
T(table.move, (1<<62))
T(table.move, 'abc', (1<<62))
T(table.move, {1,2,3}, (1<<62))
T(table.move, 5, (1<<62), (1<<62))
