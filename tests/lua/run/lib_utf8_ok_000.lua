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
show(utf8.char(72, 228, 8364, 128512)) show(utf8.len('häll€')) show(utf8.len('\xff')) show(utf8.codepoint('häl', 1, -1))
for p, c in utf8.codes('hä€') do show(p, c) end
show(utf8.offset('hä€x', 3), utf8.offset('hä€x', -1), utf8.offset('abc', 0, 2), pcall(utf8.offset, 'hä', 1, 3))
show(utf8.charpattern == '[\0-\x7F\xC2-\xFD][\x80-\xBF]*', pcall(utf8.codepoint, '\xff'))
show(utf8.len('abc', 4), pcall(utf8.len, 'abc', 5), utf8.len('', 1), utf8.char())
