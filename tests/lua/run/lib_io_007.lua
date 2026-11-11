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
T(io.open, 0)
T(io.open, 'abc', 0)
T(io.open, {1,2,3}, 0)
T(io.open, 5, 0, 0)
T(io.open, print)
T(io.open, 'abc', print)
T(io.open, {1,2,3}, print)
T(io.open, 5, print, print)
T(io.open, function() end)
T(io.open, 'abc', function() end)
T(io.open, {1,2,3}, function() end)
T(io.open, 5, function() end, function() end)
T(io.open, io.stdout)
T(io.open, 'abc', io.stdout)
