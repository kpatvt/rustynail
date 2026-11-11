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
T(pairs, {1,2,3}, '0x10')
T(pairs, 5, '0x10', '0x10')
T(pairs, 1e300)
T(pairs, 'abc', 1e300)
T(pairs, {1,2,3}, 1e300)
T(pairs, 5, 1e300, 1e300)
T(pairs, -(2^63))
T(pairs, 'abc', -(2^63))
T(pairs, {1,2,3}, -(2^63))
T(pairs, 5, -(2^63), -(2^63))
T(ipairs)
T(ipairs, nil)
T(ipairs, 'abc', nil)
T(ipairs, {1,2,3}, nil)
