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
T(os.getenv)
T(os.getenv, nil)
T(os.getenv, 'abc', nil)
T(os.getenv, {1,2,3}, nil)
T(os.getenv, 5, nil, nil)
T(os.getenv, true)
T(os.getenv, 'abc', true)
T(os.getenv, {1,2,3}, true)
T(os.getenv, 5, true, true)
T(os.getenv, {})
T(os.getenv, 'abc', {})
T(os.getenv, {1,2,3}, {})
T(os.getenv, 5, {}, {})
T(os.getenv, 'abc')
