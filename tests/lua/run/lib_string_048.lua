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
T(string.match, 'abc', 1.5)
T(string.match, {1,2,3}, 1.5)
T(string.match, 5, 1.5, 1.5)
T(string.match, 2^63)
T(string.match, 'abc', 2^63)
T(string.match, {1,2,3}, 2^63)
T(string.match, 5, 2^63, 2^63)
T(string.match, -1)
T(string.match, 'abc', -1)
T(string.match, {1,2,3}, -1)
T(string.match, 5, -1, -1)
T(string.match, 0)
T(string.match, 'abc', 0)
T(string.match, {1,2,3}, 0)
