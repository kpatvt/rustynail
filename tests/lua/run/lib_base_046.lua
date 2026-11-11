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
T(getmetatable, {1,2,3}, 1e300)
T(getmetatable, 5, 1e300, 1e300)
T(getmetatable, -(2^63))
T(getmetatable, 'abc', -(2^63))
T(getmetatable, {1,2,3}, -(2^63))
T(getmetatable, 5, -(2^63), -(2^63))
T(next)
T(next, nil)
T(next, 'abc', nil)
T(next, {1,2,3}, nil)
T(next, 5, nil, nil)
T(next, true)
T(next, 'abc', true)
T(next, {1,2,3}, true)
