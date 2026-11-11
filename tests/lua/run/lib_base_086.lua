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
T(collectgarbage, 'abc', 0)
T(collectgarbage, {1,2,3}, 0)
T(collectgarbage, 5, 0, 0)
T(collectgarbage, print)
T(collectgarbage, 'abc', print)
T(collectgarbage, {1,2,3}, print)
T(collectgarbage, 5, print, print)
T(collectgarbage, function() end)
T(collectgarbage, 'abc', function() end)
T(collectgarbage, {1,2,3}, function() end)
T(collectgarbage, 5, function() end, function() end)
T(collectgarbage, io.stdout)
T(collectgarbage, 'abc', io.stdout)
T(collectgarbage, {1,2,3}, io.stdout)
