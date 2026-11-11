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
T(math.min, 1e300)
T(math.min, 'abc', 1e300)
T(math.min, {1,2,3}, 1e300)
T(math.min, 5, 1e300, 1e300)
T(math.min, -(2^63))
T(math.min, 'abc', -(2^63))
T(math.min, {1,2,3}, -(2^63))
T(math.min, 5, -(2^63), -(2^63))
T(math.ult)
T(math.ult, nil)
T(math.ult, 'abc', nil)
T(math.ult, {1,2,3}, nil)
T(math.ult, 5, nil, nil)
T(math.ult, true)
