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
T(getmetatable, {1,2,3}, nil)
T(getmetatable, 5, nil, nil)
T(getmetatable, true)
T(getmetatable, 'abc', true)
T(getmetatable, {1,2,3}, true)
T(getmetatable, 5, true, true)
T(getmetatable, {})
T(getmetatable, 'abc', {})
T(getmetatable, {1,2,3}, {})
T(getmetatable, 5, {}, {})
T(getmetatable, 'abc')
T(getmetatable, 'abc', 'abc')
T(getmetatable, {1,2,3}, 'abc')
T(getmetatable, 5, 'abc', 'abc')
