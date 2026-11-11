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
show(('hello'):sub(2,3), ('hello'):sub(-3), ('hello'):sub(0), ('hello'):sub(10), ('hello'):sub(2,-2))
show(string.rep('ab', 3, ','), string.rep('x', 0), string.rep('', 100), string.rep('x', -1))
show(('abc'):byte(1,-1)) show(string.char(72,105)) show(('x'):byte(5))
show(('%d %5.2f %s %q %x %X %o %c %e %g %i'):format(42, 3.14159, 'hi', 'a\nb', 255, 255, 8, 65, 1.5, 0.1, 7))
show(string.format('%q', 1/3), string.format('%q', math.mininteger), string.format('%-5d|%05d|%+d', 3, 4, 5))
show(string.format('%s %s %s', nil, true, {} ~= nil), string.format('%10.3s|', 'abcdef'), string.format('%%'))
show(('hello world'):find('o w'), ('hello'):find('l+'), ('hello'):find('xyz'), ('a.b'):find('.', 1, true))
show(('key=value'):match('(%w+)=(%w+)'), ('  trim  '):match('^%s*(.-)%s*$'), ('abc'):match('()b()'))
for k, v in ('a=1, b=2, c=3'):gmatch('(%w+)=(%w+)') do show(k, v) end
show(('hello world'):gsub('o', '0'), ('abc'):gsub('%w', '%0%0'), ('abc'):gsub('', '-'), ('hello'):gsub('l', {l='L'}), ('hello'):gsub('.', function(c) if c=='e' then return false end return c:upper() end))
show(pcall(string.gsub, 'abc', '(', 'x')) show(pcall(string.gsub, 'abc', '%', 'x')) show(pcall(string.gsub, 'abc', 'b', '%2'))
show(string.pack('<i4', 100):byte(1,-1)) show(string.unpack('<i4', string.pack('<i4', -2))) show(string.packsize('i4i8'))
