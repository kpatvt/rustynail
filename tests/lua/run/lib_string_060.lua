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
T(string.gsub, 5, 1e300, 1e300)
T(string.gsub, -(2^63))
T(string.gsub, 'abc', -(2^63))
T(string.gsub, {1,2,3}, -(2^63))
T(string.gsub, 5, -(2^63), -(2^63))
T(string.pack)
T(string.pack, nil)
T(string.pack, 'abc', nil)
T(string.pack, {1,2,3}, nil)
T(string.pack, 5, nil, nil)
T(string.pack, true)
T(string.pack, 'abc', true)
T(string.pack, {1,2,3}, true)
T(string.pack, 5, true, true)
