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
T(table.pack, 'abc', 0)
T(table.pack, {1,2,3}, 0)
T(table.pack, 5, 0, 0)
T(table.pack, print)
T(table.pack, 'abc', print)
T(table.pack, {1,2,3}, print)
T(table.pack, 5, print, print)
T(table.pack, function() end)
T(table.pack, 'abc', function() end)
T(table.pack, {1,2,3}, function() end)
T(table.pack, 5, function() end, function() end)
T(table.pack, io.stdout)
T(table.pack, 'abc', io.stdout)
T(table.pack, {1,2,3}, io.stdout)
