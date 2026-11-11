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
T(string.sub, 0)
T(string.sub, 'abc', 0)
T(string.sub, {1,2,3}, 0)
T(string.sub, 5, 0, 0)
T(string.sub, print)
T(string.sub, 'abc', print)
T(string.sub, {1,2,3}, print)
T(string.sub, 5, print, print)
T(string.sub, function() end)
T(string.sub, 'abc', function() end)
T(string.sub, {1,2,3}, function() end)
T(string.sub, 5, function() end, function() end)
T(string.sub, io.stdout)
T(string.sub, 'abc', io.stdout)
