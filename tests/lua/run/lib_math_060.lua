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
T(math.fmod, 5, 1e300, 1e300)
T(math.fmod, -(2^63))
T(math.fmod, 'abc', -(2^63))
T(math.fmod, {1,2,3}, -(2^63))
T(math.fmod, 5, -(2^63), -(2^63))
T(math.modf)
T(math.modf, nil)
T(math.modf, 'abc', nil)
T(math.modf, {1,2,3}, nil)
T(math.modf, 5, nil, nil)
T(math.modf, true)
T(math.modf, 'abc', true)
T(math.modf, {1,2,3}, true)
T(math.modf, 5, true, true)
