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
T(pcall, true)
T(pcall, 'abc', true)
T(pcall, {1,2,3}, true)
T(pcall, 5, true, true)
T(pcall, {})
T(pcall, 'abc', {})
T(pcall, {1,2,3}, {})
T(pcall, 5, {}, {})
T(pcall, 'abc')
T(pcall, 'abc', 'abc')
T(pcall, {1,2,3}, 'abc')
T(pcall, 5, 'abc', 'abc')
T(pcall, '10')
T(pcall, 'abc', '10')
