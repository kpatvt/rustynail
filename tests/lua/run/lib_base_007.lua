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
T(tostring, 0)
T(tostring, 'abc', 0)
T(tostring, {1,2,3}, 0)
T(tostring, 5, 0, 0)
T(tostring, print)
T(tostring, 'abc', print)
T(tostring, {1,2,3}, print)
T(tostring, 5, print, print)
T(tostring, function() end)
T(tostring, 'abc', function() end)
T(tostring, {1,2,3}, function() end)
T(tostring, 5, function() end, function() end)
T(tostring, io.stdout)
T(tostring, 'abc', io.stdout)
