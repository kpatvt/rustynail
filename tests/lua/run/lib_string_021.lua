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
T(string.rep, 'abc', 0)
T(string.rep, {1,2,3}, 0)
T(string.rep, 5, 0, 0)
T(string.rep, print)
T(string.rep, 'abc', print)
T(string.rep, {1,2,3}, print)
T(string.rep, 5, print, print)
T(string.rep, function() end)
T(string.rep, 'abc', function() end)
T(string.rep, {1,2,3}, function() end)
T(string.rep, 5, function() end, function() end)
T(string.rep, io.stdout)
T(string.rep, 'abc', io.stdout)
T(string.rep, {1,2,3}, io.stdout)
