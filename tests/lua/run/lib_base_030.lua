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
T(rawequal, 'abc', -1)
T(rawequal, {1,2,3}, -1)
T(rawequal, 5, -1, -1)
T(rawequal, 0)
T(rawequal, 'abc', 0)
T(rawequal, {1,2,3}, 0)
T(rawequal, 5, 0, 0)
T(rawequal, print)
T(rawequal, 'abc', print)
T(rawequal, {1,2,3}, print)
T(rawequal, 5, print, print)
T(rawequal, function() end)
T(rawequal, 'abc', function() end)
T(rawequal, {1,2,3}, function() end)
