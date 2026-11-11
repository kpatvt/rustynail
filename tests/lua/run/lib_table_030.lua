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
T(table.sort, 'abc', -1)
T(table.sort, {1,2,3}, -1)
T(table.sort, 5, -1, -1)
T(table.sort, 0)
T(table.sort, 'abc', 0)
T(table.sort, {1,2,3}, 0)
T(table.sort, 5, 0, 0)
T(table.sort, print)
T(table.sort, 'abc', print)
T(table.sort, {1,2,3}, print)
T(table.sort, 5, print, print)
T(table.sort, function() end)
T(table.sort, 'abc', function() end)
T(table.sort, {1,2,3}, function() end)
