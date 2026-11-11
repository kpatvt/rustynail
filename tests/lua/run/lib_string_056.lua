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
T(string.gsub, 5, nil, nil)
T(string.gsub, true)
T(string.gsub, 'abc', true)
T(string.gsub, {1,2,3}, true)
T(string.gsub, 5, true, true)
T(string.gsub, {})
T(string.gsub, 'abc', {})
T(string.gsub, {1,2,3}, {})
T(string.gsub, 5, {}, {})
T(string.gsub, 'abc')
T(string.gsub, 'abc', 'abc')
T(string.gsub, {1,2,3}, 'abc')
T(string.gsub, 5, 'abc', 'abc')
T(string.gsub, '10')
