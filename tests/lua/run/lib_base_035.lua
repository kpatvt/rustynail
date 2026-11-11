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
T(rawlen, {1,2,3}, 0)
T(rawlen, 5, 0, 0)
T(rawlen, print)
T(rawlen, 'abc', print)
T(rawlen, {1,2,3}, print)
T(rawlen, 5, print, print)
T(rawlen, function() end)
T(rawlen, 'abc', function() end)
T(rawlen, {1,2,3}, function() end)
T(rawlen, 5, function() end, function() end)
T(rawlen, io.stdout)
T(rawlen, 'abc', io.stdout)
T(rawlen, {1,2,3}, io.stdout)
T(rawlen, 5, io.stdout, io.stdout)
