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
T(math.ceil, 0)
T(math.ceil, 'abc', 0)
T(math.ceil, {1,2,3}, 0)
T(math.ceil, 5, 0, 0)
T(math.ceil, print)
T(math.ceil, 'abc', print)
T(math.ceil, {1,2,3}, print)
T(math.ceil, 5, print, print)
T(math.ceil, function() end)
T(math.ceil, 'abc', function() end)
T(math.ceil, {1,2,3}, function() end)
T(math.ceil, 5, function() end, function() end)
T(math.ceil, io.stdout)
T(math.ceil, 'abc', io.stdout)
