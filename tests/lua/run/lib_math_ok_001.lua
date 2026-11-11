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
show(math.tointeger(2^53), 2^53 == 2^53 + 1, math.maxinteger + 1 == math.mininteger, math.maxinteger // -1)
show(string.format('%.3f', math.atan(1, 2)), string.format('%.3f', math.asin(0.5)), string.format('%.3f', math.acos(0.5)), string.format('%.3f', math.tan(1)))
