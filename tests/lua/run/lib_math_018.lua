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
T(math.sqrt, 1e300)
T(math.sqrt, 'abc', 1e300)
T(math.sqrt, {1,2,3}, 1e300)
T(math.sqrt, 5, 1e300, 1e300)
T(math.sqrt, -(2^63))
T(math.sqrt, 'abc', -(2^63))
T(math.sqrt, {1,2,3}, -(2^63))
T(math.sqrt, 5, -(2^63), -(2^63))
T(math.sin)
T(math.sin, nil)
T(math.sin, 'abc', nil)
T(math.sin, {1,2,3}, nil)
T(math.sin, 5, nil, nil)
T(math.sin, true)
