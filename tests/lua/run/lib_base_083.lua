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
T(load, 1e300)
T(load, 'abc', 1e300)
T(load, {1,2,3}, 1e300)
T(load, 5, 1e300, 1e300)
T(load, -(2^63))
T(load, 'abc', -(2^63))
T(load, {1,2,3}, -(2^63))
T(load, 5, -(2^63), -(2^63))
T(collectgarbage)
T(collectgarbage, nil)
T(collectgarbage, 'abc', nil)
T(collectgarbage, {1,2,3}, nil)
T(collectgarbage, 5, nil, nil)
T(collectgarbage, true)
