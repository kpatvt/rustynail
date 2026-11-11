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
T(math.ult, 'abc', 0)
T(math.ult, {1,2,3}, 0)
T(math.ult, 5, 0, 0)
T(math.ult, print)
T(math.ult, 'abc', print)
T(math.ult, {1,2,3}, print)
T(math.ult, 5, print, print)
T(math.ult, function() end)
T(math.ult, 'abc', function() end)
T(math.ult, {1,2,3}, function() end)
T(math.ult, 5, function() end, function() end)
T(math.ult, io.stdout)
T(math.ult, 'abc', io.stdout)
T(math.ult, {1,2,3}, io.stdout)
