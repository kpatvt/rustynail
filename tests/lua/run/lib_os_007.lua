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
T(os.difftime, 0)
T(os.difftime, 'abc', 0)
T(os.difftime, {1,2,3}, 0)
T(os.difftime, 5, 0, 0)
T(os.difftime, print)
T(os.difftime, 'abc', print)
T(os.difftime, {1,2,3}, print)
T(os.difftime, 5, print, print)
T(os.difftime, function() end)
T(os.difftime, 'abc', function() end)
T(os.difftime, {1,2,3}, function() end)
T(os.difftime, 5, function() end, function() end)
T(os.difftime, io.stdout)
T(os.difftime, 'abc', io.stdout)
