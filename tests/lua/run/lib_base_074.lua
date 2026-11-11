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
T(pcall, -(2^63))
T(pcall, 'abc', -(2^63))
T(pcall, {1,2,3}, -(2^63))
T(pcall, 5, -(2^63), -(2^63))
T(xpcall)
T(xpcall, nil)
T(xpcall, 'abc', nil)
T(xpcall, {1,2,3}, nil)
T(xpcall, 5, nil, nil)
T(xpcall, true)
T(xpcall, 'abc', true)
T(xpcall, {1,2,3}, true)
T(xpcall, 5, true, true)
T(xpcall, {})
