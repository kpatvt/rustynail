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
local n = os.tmpname() local f = assert(io.open(n, 'w')) show(f:write('line1\nline2\n', 3, ' ', 4.5, '\n')) f:close() for l in io.lines(n) do show(l) end os.remove(n)
local n = os.tmpname() local f = io.open(n, 'w') f:write('12 0x1F -3.5e2 abc\nrest') f:close() f = io.open(n) show(f:read('n', 'n', 'n', 'n')) show(f:read('l')) show(f:read('a')) show(f:read('a'), f:read('l'), f:read(0)) f:close() os.remove(n)
local n = os.tmpname() local f = io.open(n, 'w+') f:write('hello world') show(f:seek('set', 6), f:read(3), f:seek('cur'), f:seek('end'), f:seek('set')) f:close() os.remove(n)
local n = os.tmpname() local f = io.open(n, 'wb') f:setvbuf('full', 1024) f:write('x') show(f:flush() == f, io.type(f), io.type(5)) f:close() show(io.type(f), tostring(f)) os.remove(n)
show(io.open('/nonexistent/x', 'r')) show(pcall(io.open, 'x', 'bad')) show(pcall(io.lines, '/nonexistent/x'))
local n = os.tmpname() local f = io.open(n, 'w') f:write('a\nb\nc') f:close() local t = {} for a, b in io.lines(n, 1, 'l') do t[#t+1] = a .. '/' .. tostring(b) end show(table.concat(t, ';')) os.remove(n)
local n = os.tmpname() local f = io.open(n, 'w') f:write('x') f:close() local g = io.open(n) show(pcall(g.write, g, 'y')) show(g:read(5), g:read(1)) g:close() show(pcall(g.read, g)) os.remove(n)
show(io.write('') == io.stdout, io.output() == io.stdout, io.input() == io.stdin, io.stdout:seek('cur') ~= false)
local n = os.tmpname() io.output(n) io.write('via default') io.close() io.output(io.stdout) io.input(n) show(io.read('a')) io.close(io.input()) io.input(io.stdin) os.remove(n)
local p = io.popen('echo hi', 'r') show(p:read('a')) show(p:close()) local q = io.popen('exit 2') show(q:close())
local f = io.tmpfile() f:write('tmp') f:seek('set') show(f:read('a')) f:close()
local n = os.tmpname() local f = io.open(n, 'w') f:write('1\n2\n') f:close() for l in io.open(n):lines('n') do show(l) end os.remove(n)
