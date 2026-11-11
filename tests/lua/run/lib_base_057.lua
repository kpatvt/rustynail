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
T(ipairs, 'abc', '10')
T(ipairs, {1,2,3}, '10')
T(ipairs, 5, '10', '10')
T(ipairs, 1.5)
T(ipairs, 'abc', 1.5)
T(ipairs, {1,2,3}, 1.5)
T(ipairs, 5, 1.5, 1.5)
T(ipairs, 2^63)
T(ipairs, 'abc', 2^63)
T(ipairs, {1,2,3}, 2^63)
T(ipairs, 5, 2^63, 2^63)
T(ipairs, -1)
T(ipairs, 'abc', -1)
T(ipairs, {1,2,3}, -1)
