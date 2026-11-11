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
T(os.getenv, 5, '0x10', '0x10')
T(os.getenv, 1e300)
T(os.getenv, 'abc', 1e300)
T(os.getenv, {1,2,3}, 1e300)
T(os.getenv, 5, 1e300, 1e300)
T(os.getenv, -(2^63))
T(os.getenv, 'abc', -(2^63))
T(os.getenv, {1,2,3}, -(2^63))
T(os.getenv, 5, -(2^63), -(2^63))
T(os.difftime)
T(os.difftime, nil)
T(os.difftime, 'abc', nil)
T(os.difftime, {1,2,3}, nil)
T(os.difftime, 5, nil, nil)
