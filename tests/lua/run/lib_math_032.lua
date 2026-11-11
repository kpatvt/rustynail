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
T(math.tan, 'abc', 1e300)
T(math.tan, {1,2,3}, 1e300)
T(math.tan, 5, 1e300, 1e300)
T(math.tan, -(2^63))
T(math.tan, 'abc', -(2^63))
T(math.tan, {1,2,3}, -(2^63))
T(math.tan, 5, -(2^63), -(2^63))
T(math.asin)
T(math.asin, nil)
T(math.asin, 'abc', nil)
T(math.asin, {1,2,3}, nil)
T(math.asin, 5, nil, nil)
T(math.asin, true)
T(math.asin, 'abc', true)
