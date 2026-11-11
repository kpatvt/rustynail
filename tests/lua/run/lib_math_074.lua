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
T(math.type, -(2^63))
T(math.type, 'abc', -(2^63))
T(math.type, {1,2,3}, -(2^63))
T(math.type, 5, -(2^63), -(2^63))
T(math.max)
T(math.max, nil)
T(math.max, 'abc', nil)
T(math.max, {1,2,3}, nil)
T(math.max, 5, nil, nil)
T(math.max, true)
T(math.max, 'abc', true)
T(math.max, {1,2,3}, true)
T(math.max, 5, true, true)
T(math.max, {})
