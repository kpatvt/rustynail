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
T(math.fmod, 5, -1, -1)
T(math.fmod, 0)
T(math.fmod, 'abc', 0)
T(math.fmod, {1,2,3}, 0)
T(math.fmod, 5, 0, 0)
T(math.fmod, print)
T(math.fmod, 'abc', print)
T(math.fmod, {1,2,3}, print)
T(math.fmod, 5, print, print)
T(math.fmod, function() end)
T(math.fmod, 'abc', function() end)
T(math.fmod, {1,2,3}, function() end)
T(math.fmod, 5, function() end, function() end)
T(math.fmod, io.stdout)
