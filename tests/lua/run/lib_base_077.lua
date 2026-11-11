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
T(xpcall, 'abc', print)
T(xpcall, {1,2,3}, print)
T(xpcall, 5, print, print)
T(xpcall, function() end)
T(xpcall, 'abc', function() end)
T(xpcall, {1,2,3}, function() end)
T(xpcall, 5, function() end, function() end)
T(xpcall, io.stdout)
T(xpcall, 'abc', io.stdout)
T(xpcall, {1,2,3}, io.stdout)
T(xpcall, 5, io.stdout, io.stdout)
T(xpcall, (1<<62))
T(xpcall, 'abc', (1<<62))
T(xpcall, {1,2,3}, (1<<62))
