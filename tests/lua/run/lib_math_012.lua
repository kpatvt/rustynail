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
T(math.floor, 'abc', print)
T(math.floor, {1,2,3}, print)
T(math.floor, 5, print, print)
T(math.floor, function() end)
T(math.floor, 'abc', function() end)
T(math.floor, {1,2,3}, function() end)
T(math.floor, 5, function() end, function() end)
T(math.floor, io.stdout)
T(math.floor, 'abc', io.stdout)
T(math.floor, {1,2,3}, io.stdout)
T(math.floor, 5, io.stdout, io.stdout)
T(math.floor, (1<<62))
T(math.floor, 'abc', (1<<62))
T(math.floor, {1,2,3}, (1<<62))
