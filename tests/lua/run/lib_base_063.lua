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
T(assert, print)
T(assert, 'abc', print)
T(assert, {1,2,3}, print)
T(assert, 5, print, print)
T(assert, function() end)
T(assert, 'abc', function() end)
T(assert, {1,2,3}, function() end)
T(assert, 5, function() end, function() end)
T(assert, io.stdout)
T(assert, 'abc', io.stdout)
T(assert, {1,2,3}, io.stdout)
T(assert, 5, io.stdout, io.stdout)
T(assert, (1<<62))
T(assert, 'abc', (1<<62))
