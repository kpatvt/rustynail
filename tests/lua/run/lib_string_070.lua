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
T(string.packsize, true)
T(string.packsize, 'abc', true)
T(string.packsize, {1,2,3}, true)
T(string.packsize, 5, true, true)
T(string.packsize, {})
T(string.packsize, 'abc', {})
T(string.packsize, {1,2,3}, {})
T(string.packsize, 5, {}, {})
T(string.packsize, 'abc')
T(string.packsize, 'abc', 'abc')
T(string.packsize, {1,2,3}, 'abc')
T(string.packsize, 5, 'abc', 'abc')
T(string.packsize, '10')
T(string.packsize, 'abc', '10')
