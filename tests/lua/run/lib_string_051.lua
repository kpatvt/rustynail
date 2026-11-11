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
T(string.match, 5, -(2^63), -(2^63))
T(string.gmatch)
T(string.gmatch, nil)
T(string.gmatch, 'abc', nil)
T(string.gmatch, {1,2,3}, nil)
T(string.gmatch, 5, nil, nil)
T(string.gmatch, true)
T(string.gmatch, 'abc', true)
T(string.gmatch, {1,2,3}, true)
T(string.gmatch, 5, true, true)
T(string.gmatch, {})
T(string.gmatch, 'abc', {})
T(string.gmatch, {1,2,3}, {})
T(string.gmatch, 5, {}, {})
