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
T(math.sin, 'abc', -(2^63))
T(math.sin, {1,2,3}, -(2^63))
T(math.sin, 5, -(2^63), -(2^63))
T(math.cos)
T(math.cos, nil)
T(math.cos, 'abc', nil)
T(math.cos, {1,2,3}, nil)
T(math.cos, 5, nil, nil)
T(math.cos, true)
T(math.cos, 'abc', true)
T(math.cos, {1,2,3}, true)
T(math.cos, 5, true, true)
T(math.cos, {})
T(math.cos, 'abc', {})
