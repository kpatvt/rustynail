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
T(io.type, 5, 2^63, 2^63)
T(io.type, -1)
T(io.type, 'abc', -1)
T(io.type, {1,2,3}, -1)
T(io.type, 5, -1, -1)
T(io.type, 0)
T(io.type, 'abc', 0)
T(io.type, {1,2,3}, 0)
T(io.type, 5, 0, 0)
T(io.type, print)
T(io.type, 'abc', print)
T(io.type, {1,2,3}, print)
T(io.type, 5, print, print)
T(io.type, function() end)
