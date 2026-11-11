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
T(rawget, 'abc', -(2^63))
T(rawget, {1,2,3}, -(2^63))
T(rawget, 5, -(2^63), -(2^63))
T(rawset)
T(rawset, nil)
T(rawset, 'abc', nil)
T(rawset, {1,2,3}, nil)
T(rawset, 5, nil, nil)
T(rawset, true)
T(rawset, 'abc', true)
T(rawset, {1,2,3}, true)
T(rawset, 5, true, true)
T(rawset, {})
T(rawset, 'abc', {})
