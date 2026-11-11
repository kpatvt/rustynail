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
T(math.type, 0)
T(math.type, 'abc', 0)
T(math.type, {1,2,3}, 0)
T(math.type, 5, 0, 0)
T(math.type, print)
T(math.type, 'abc', print)
T(math.type, {1,2,3}, print)
T(math.type, 5, print, print)
T(math.type, function() end)
T(math.type, 'abc', function() end)
T(math.type, {1,2,3}, function() end)
T(math.type, 5, function() end, function() end)
T(math.type, io.stdout)
T(math.type, 'abc', io.stdout)
