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
T(string.gsub, 5, -1, -1)
T(string.gsub, 0)
T(string.gsub, 'abc', 0)
T(string.gsub, {1,2,3}, 0)
T(string.gsub, 5, 0, 0)
T(string.gsub, print)
T(string.gsub, 'abc', print)
T(string.gsub, {1,2,3}, print)
T(string.gsub, 5, print, print)
T(string.gsub, function() end)
T(string.gsub, 'abc', function() end)
T(string.gsub, {1,2,3}, function() end)
T(string.gsub, 5, function() end, function() end)
T(string.gsub, io.stdout)
