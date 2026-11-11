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
T(io.read, 'abc', -(2^63))
T(io.read, {1,2,3}, -(2^63))
T(io.read, 5, -(2^63), -(2^63))
T(io.write)
T(io.write, nil)
T(io.write, 'abc', nil)
T(io.write, {1,2,3}, nil)
T(io.write, 5, nil, nil)
T(io.write, true)
T(io.write, 'abc', true)
T(io.write, {1,2,3}, true)
T(io.write, 5, true, true)
T(io.write, {})
T(io.write, 'abc', {})
