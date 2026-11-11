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
T(string.rep, 'abc', -(2^63))
T(string.rep, {1,2,3}, -(2^63))
T(string.rep, 5, -(2^63), -(2^63))
T(string.reverse)
T(string.reverse, nil)
T(string.reverse, 'abc', nil)
T(string.reverse, {1,2,3}, nil)
T(string.reverse, 5, nil, nil)
T(string.reverse, true)
T(string.reverse, 'abc', true)
T(string.reverse, {1,2,3}, true)
T(string.reverse, 5, true, true)
T(string.reverse, {})
T(string.reverse, 'abc', {})
