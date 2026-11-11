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
T(string.char, {1,2,3}, 0)
T(string.char, 5, 0, 0)
T(string.char, print)
T(string.char, 'abc', print)
T(string.char, {1,2,3}, print)
T(string.char, 5, print, print)
T(string.char, function() end)
T(string.char, 'abc', function() end)
T(string.char, {1,2,3}, function() end)
T(string.char, 5, function() end, function() end)
T(string.char, io.stdout)
T(string.char, 'abc', io.stdout)
T(string.char, {1,2,3}, io.stdout)
T(string.char, 5, io.stdout, io.stdout)
