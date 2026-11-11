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
T(collectgarbage, 'abc', true)
T(collectgarbage, {1,2,3}, true)
T(collectgarbage, 5, true, true)
T(collectgarbage, {})
T(collectgarbage, 'abc', {})
T(collectgarbage, {1,2,3}, {})
T(collectgarbage, 5, {}, {})
T(collectgarbage, 'abc')
T(collectgarbage, 'abc', 'abc')
T(collectgarbage, {1,2,3}, 'abc')
T(collectgarbage, 5, 'abc', 'abc')
T(collectgarbage, '10')
T(collectgarbage, 'abc', '10')
T(collectgarbage, {1,2,3}, '10')
