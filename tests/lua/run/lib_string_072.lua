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
T(string.packsize, 0)
T(string.packsize, 'abc', 0)
T(string.packsize, {1,2,3}, 0)
T(string.packsize, 5, 0, 0)
T(string.packsize, print)
T(string.packsize, 'abc', print)
T(string.packsize, {1,2,3}, print)
T(string.packsize, 5, print, print)
T(string.packsize, function() end)
T(string.packsize, 'abc', function() end)
T(string.packsize, {1,2,3}, function() end)
T(string.packsize, 5, function() end, function() end)
T(string.packsize, io.stdout)
T(string.packsize, 'abc', io.stdout)
