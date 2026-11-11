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
T(os.difftime, true)
T(os.difftime, 'abc', true)
T(os.difftime, {1,2,3}, true)
T(os.difftime, 5, true, true)
T(os.difftime, {})
T(os.difftime, 'abc', {})
T(os.difftime, {1,2,3}, {})
T(os.difftime, 5, {}, {})
T(os.difftime, 'abc')
T(os.difftime, 'abc', 'abc')
T(os.difftime, {1,2,3}, 'abc')
T(os.difftime, 5, 'abc', 'abc')
T(os.difftime, '10')
T(os.difftime, 'abc', '10')
