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
T(math.asin, {1,2,3}, true)
T(math.asin, 5, true, true)
T(math.asin, {})
T(math.asin, 'abc', {})
T(math.asin, {1,2,3}, {})
T(math.asin, 5, {}, {})
T(math.asin, 'abc')
T(math.asin, 'abc', 'abc')
T(math.asin, {1,2,3}, 'abc')
T(math.asin, 5, 'abc', 'abc')
T(math.asin, '10')
T(math.asin, 'abc', '10')
T(math.asin, {1,2,3}, '10')
T(math.asin, 5, '10', '10')
