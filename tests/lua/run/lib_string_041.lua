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
T(string.format, 'abc', '0x10')
T(string.format, {1,2,3}, '0x10')
T(string.format, 5, '0x10', '0x10')
T(string.format, 1e300)
T(string.format, 'abc', 1e300)
T(string.format, {1,2,3}, 1e300)
T(string.format, 5, 1e300, 1e300)
T(string.format, -(2^63))
T(string.format, 'abc', -(2^63))
T(string.format, {1,2,3}, -(2^63))
T(string.format, 5, -(2^63), -(2^63))
T(string.find)
T(string.find, nil)
T(string.find, 'abc', nil)
