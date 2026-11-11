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
T(io.open, -(2^63))
T(io.open, 'abc', -(2^63))
T(io.open, {1,2,3}, -(2^63))
T(io.open, 5, -(2^63), -(2^63))
T(io.lines)
T(io.lines, nil)
T(io.lines, 'abc', nil)
T(io.lines, {1,2,3}, nil)
T(io.lines, 5, nil, nil)
T(io.lines, true)
T(io.lines, 'abc', true)
T(io.lines, {1,2,3}, true)
T(io.lines, 5, true, true)
T(io.lines, {})
