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
T(next, 5, 0, 0)
T(next, print)
T(next, 'abc', print)
T(next, {1,2,3}, print)
T(next, 5, print, print)
T(next, function() end)
T(next, 'abc', function() end)
T(next, {1,2,3}, function() end)
T(next, 5, function() end, function() end)
T(next, io.stdout)
T(next, 'abc', io.stdout)
T(next, {1,2,3}, io.stdout)
T(next, 5, io.stdout, io.stdout)
T(next, (1<<62))
