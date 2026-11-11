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
T(rawget, 'abc', 0)
T(rawget, {1,2,3}, 0)
T(rawget, 5, 0, 0)
T(rawget, print)
T(rawget, 'abc', print)
T(rawget, {1,2,3}, print)
T(rawget, 5, print, print)
T(rawget, function() end)
T(rawget, 'abc', function() end)
T(rawget, {1,2,3}, function() end)
T(rawget, 5, function() end, function() end)
T(rawget, io.stdout)
T(rawget, 'abc', io.stdout)
T(rawget, {1,2,3}, io.stdout)
