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
T(setmetatable, 5, print, print)
T(setmetatable, function() end)
T(setmetatable, 'abc', function() end)
T(setmetatable, {1,2,3}, function() end)
T(setmetatable, 5, function() end, function() end)
T(setmetatable, io.stdout)
T(setmetatable, 'abc', io.stdout)
T(setmetatable, {1,2,3}, io.stdout)
T(setmetatable, 5, io.stdout, io.stdout)
T(setmetatable, (1<<62))
T(setmetatable, 'abc', (1<<62))
T(setmetatable, {1,2,3}, (1<<62))
T(setmetatable, 5, (1<<62), (1<<62))
T(setmetatable, '0x10')
