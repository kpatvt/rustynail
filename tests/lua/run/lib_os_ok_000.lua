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
local d = os.date('!*t', 86400) show(d.year, d.month, d.day, d.hour, d.isdst) show(os.date('!%Y-%m-%d %H:%M:%S', 3600))
show(os.time{year=2020, month=1, day=1, hour=12}, os.time{year=2020, month=14, day=35, hour=0} ~= nil, pcall(os.time, {year=2020}))
show(os.difftime(10, 5), pcall(os.difftime), type(os.clock()), os.getenv('RN_NOPE_XYZ'), type(os.getenv('PATH')))
show(pcall(os.date, '%Ez')) show(pcall(os.date, '%Q')) show(os.date('!%c', 0)) show(os.date('!%x %X %p', 0))
show(os.setlocale(), os.setlocale('C'), os.setlocale(nil, 'all'), os.setlocale('nonexistent_locale_zz'), pcall(os.setlocale, 'C', 'bogus'))
show(os.remove('/nonexistent/zzz')) show(os.rename('/nonexistent/a', '/nonexistent/b'))
show(os.execute() ~= nil, os.execute('exit 3')) show(os.execute('true'))
local n = os.tmpname() show(type(n)) os.remove(n)
