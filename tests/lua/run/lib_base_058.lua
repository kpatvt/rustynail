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
T(ipairs, 5, -1, -1)
T(ipairs, 0)
T(ipairs, 'abc', 0)
T(ipairs, {1,2,3}, 0)
T(ipairs, 5, 0, 0)
T(ipairs, print)
T(ipairs, 'abc', print)
T(ipairs, {1,2,3}, print)
T(ipairs, 5, print, print)
T(ipairs, function() end)
T(ipairs, 'abc', function() end)
T(ipairs, {1,2,3}, function() end)
T(ipairs, 5, function() end, function() end)
T(ipairs, io.stdout)
