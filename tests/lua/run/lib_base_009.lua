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
T(tostring, -(2^63))
T(tostring, 'abc', -(2^63))
T(tostring, {1,2,3}, -(2^63))
T(tostring, 5, -(2^63), -(2^63))
T(tonumber)
T(tonumber, nil)
T(tonumber, 'abc', nil)
T(tonumber, {1,2,3}, nil)
T(tonumber, 5, nil, nil)
T(tonumber, true)
T(tonumber, 'abc', true)
T(tonumber, {1,2,3}, true)
T(tonumber, 5, true, true)
T(tonumber, {})
