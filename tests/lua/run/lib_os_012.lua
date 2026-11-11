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
T(os.setlocale, 'abc', print)
T(os.setlocale, {1,2,3}, print)
T(os.setlocale, 5, print, print)
T(os.setlocale, function() end)
T(os.setlocale, 'abc', function() end)
T(os.setlocale, {1,2,3}, function() end)
T(os.setlocale, 5, function() end, function() end)
T(os.setlocale, io.stdout)
T(os.setlocale, 'abc', io.stdout)
T(os.setlocale, {1,2,3}, io.stdout)
T(os.setlocale, 5, io.stdout, io.stdout)
T(os.setlocale, (1<<62))
T(os.setlocale, 'abc', (1<<62))
T(os.setlocale, {1,2,3}, (1<<62))
