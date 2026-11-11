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
T(table.unpack, -1)
T(table.unpack, 'abc', -1)
T(table.unpack, {1,2,3}, -1)
T(table.unpack, 5, -1, -1)
T(table.unpack, 0)
T(table.unpack, 'abc', 0)
T(table.unpack, {1,2,3}, 0)
T(table.unpack, 5, 0, 0)
T(table.unpack, print)
T(table.unpack, 'abc', print)
T(table.unpack, {1,2,3}, print)
T(table.unpack, 5, print, print)
T(table.unpack, function() end)
T(table.unpack, 'abc', function() end)
