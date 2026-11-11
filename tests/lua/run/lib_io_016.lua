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
T(io.close, -1)
T(io.close, 'abc', -1)
T(io.close, {1,2,3}, -1)
T(io.close, 5, -1, -1)
T(io.close, 0)
T(io.close, 'abc', 0)
T(io.close, {1,2,3}, 0)
T(io.close, 5, 0, 0)
T(io.close, print)
T(io.close, 'abc', print)
T(io.close, {1,2,3}, print)
T(io.close, 5, print, print)
T(io.close, function() end)
T(io.close, 'abc', function() end)
