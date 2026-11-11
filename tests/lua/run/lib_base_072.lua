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
T(pcall, 0)
T(pcall, 'abc', 0)
T(pcall, {1,2,3}, 0)
T(pcall, 5, 0, 0)
T(pcall, print)
T(pcall, 'abc', print)
T(pcall, {1,2,3}, print)
T(pcall, 5, print, print)
T(pcall, function() end)
T(pcall, 'abc', function() end)
T(pcall, {1,2,3}, function() end)
T(pcall, 5, function() end, function() end)
T(pcall, io.stdout)
T(pcall, 'abc', io.stdout)
