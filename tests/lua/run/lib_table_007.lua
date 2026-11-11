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
T(table.remove, 0)
T(table.remove, 'abc', 0)
T(table.remove, {1,2,3}, 0)
T(table.remove, 5, 0, 0)
T(table.remove, print)
T(table.remove, 'abc', print)
T(table.remove, {1,2,3}, print)
T(table.remove, 5, print, print)
T(table.remove, function() end)
T(table.remove, 'abc', function() end)
T(table.remove, {1,2,3}, function() end)
T(table.remove, 5, function() end, function() end)
T(table.remove, io.stdout)
T(table.remove, 'abc', io.stdout)
