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
T(math.sin, 'abc', 0)
T(math.sin, {1,2,3}, 0)
T(math.sin, 5, 0, 0)
T(math.sin, print)
T(math.sin, 'abc', print)
T(math.sin, {1,2,3}, print)
T(math.sin, 5, print, print)
T(math.sin, function() end)
T(math.sin, 'abc', function() end)
T(math.sin, {1,2,3}, function() end)
T(math.sin, 5, function() end, function() end)
T(math.sin, io.stdout)
T(math.sin, 'abc', io.stdout)
T(math.sin, {1,2,3}, io.stdout)
