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
T(string.rep, 'abc', true)
T(string.rep, {1,2,3}, true)
T(string.rep, 5, true, true)
T(string.rep, {})
T(string.rep, 'abc', {})
T(string.rep, {1,2,3}, {})
T(string.rep, 5, {}, {})
T(string.rep, 'abc')
T(string.rep, 'abc', 'abc')
T(string.rep, {1,2,3}, 'abc')
T(string.rep, 5, 'abc', 'abc')
T(string.rep, '10')
T(string.rep, 'abc', '10')
T(string.rep, {1,2,3}, '10')
