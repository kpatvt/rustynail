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
T(io.lines, 'abc', print)
T(io.lines, {1,2,3}, print)
T(io.lines, 5, print, print)
T(io.lines, function() end)
T(io.lines, 'abc', function() end)
T(io.lines, {1,2,3}, function() end)
T(io.lines, 5, function() end, function() end)
T(io.lines, io.stdout)
T(io.lines, 'abc', io.stdout)
T(io.lines, {1,2,3}, io.stdout)
T(io.lines, 5, io.stdout, io.stdout)
T(io.lines, (1<<62))
T(io.lines, 'abc', (1<<62))
T(io.lines, {1,2,3}, (1<<62))
