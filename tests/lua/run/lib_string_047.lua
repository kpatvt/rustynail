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
T(string.match, 5, true, true)
T(string.match, {})
T(string.match, 'abc', {})
T(string.match, {1,2,3}, {})
T(string.match, 5, {}, {})
T(string.match, 'abc')
T(string.match, 'abc', 'abc')
T(string.match, {1,2,3}, 'abc')
T(string.match, 5, 'abc', 'abc')
T(string.match, '10')
T(string.match, 'abc', '10')
T(string.match, {1,2,3}, '10')
T(string.match, 5, '10', '10')
T(string.match, 1.5)
