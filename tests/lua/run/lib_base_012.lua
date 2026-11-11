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
T(tonumber, 'abc', print)
T(tonumber, {1,2,3}, print)
T(tonumber, 5, print, print)
T(tonumber, function() end)
T(tonumber, 'abc', function() end)
T(tonumber, {1,2,3}, function() end)
T(tonumber, 5, function() end, function() end)
T(tonumber, io.stdout)
T(tonumber, 'abc', io.stdout)
T(tonumber, {1,2,3}, io.stdout)
T(tonumber, 5, io.stdout, io.stdout)
T(tonumber, (1<<62))
T(tonumber, 'abc', (1<<62))
T(tonumber, {1,2,3}, (1<<62))
