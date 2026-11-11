#!/usr/bin/env python3
"""Generate tests/lua/run/lib_*.lua: library-level scripts compared against the reference by
`cargo xtask diff-run`. Each script prints the results of pcall'd snippets (deterministic output)."""
import os, itertools
OUT = os.path.join(os.path.dirname(__file__), "..", "tests", "lua", "run")
os.makedirs(OUT, exist_ok=True)

HEAD = """local function show(...)
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
"""

BAD = ["nil", "true", "{}", "'abc'", "'10'", "1.5", "2^63", "-1", "0", "print", "function() end", "io.stdout", "(1<<62)", "'0x10'", "1e300", "-(2^63)"]

snips = {}
def add(lib, s): snips.setdefault(lib, []).append(s)

# --- generic bad-argument sweeps per function
funcs = {
 "string": ["len","sub","upper","lower","rep","reverse","byte","char","format","find","match","gmatch","gsub","pack","unpack","packsize"],
 "table": ["insert","remove","concat","unpack","pack","move","sort"],
 "math": ["abs","ceil","floor","sqrt","sin","cos","tan","asin","acos","atan","exp","log","fmod","modf","tointeger","type","max","min","ult"] ,
 "utf8": ["char","codepoint","len","offset","codes"],
 "os": ["getenv","difftime","setlocale"],
 "base": ["type","tostring","tonumber","select","rawget","rawset","rawequal","rawlen","setmetatable","getmetatable","next","pairs","ipairs","assert","error","pcall","xpcall","load","collectgarbage"],
 "io": ["type","open","lines","close","read","write"],
}
for lib, fs in funcs.items():
    for f in fs:
        ref = f if lib == "base" else f"{lib}.{f}"
        add(lib, f"T({ref})")
        for b in BAD:
            add(lib, f"T({ref}, {b})")
            add(lib, f"T({ref}, 'abc', {b})")
            add(lib, f"T({ref}, {{1,2,3}}, {b})")
            add(lib, f"T({ref}, 5, {b}, {b})")

# --- hand-written behaviors
S = lambda s: add("string_ok", s)
for s in [
 "show(('hello'):sub(2,3), ('hello'):sub(-3), ('hello'):sub(0), ('hello'):sub(10), ('hello'):sub(2,-2))",
 "show(string.rep('ab', 3, ','), string.rep('x', 0), string.rep('', 100), string.rep('x', -1))",
 "show(('abc'):byte(1,-1)) show(string.char(72,105)) show(('x'):byte(5))",
 "show(('%d %5.2f %s %q %x %X %o %c %e %g %i'):format(42, 3.14159, 'hi', 'a\\nb', 255, 255, 8, 65, 1.5, 0.1, 7))",
 "show(string.format('%q', 1/3), string.format('%q', math.mininteger), string.format('%-5d|%05d|%+d', 3, 4, 5))",
 "show(string.format('%s %s %s', nil, true, {} ~= nil), string.format('%10.3s|', 'abcdef'), string.format('%%'))",
 "show(('hello world'):find('o w'), ('hello'):find('l+'), ('hello'):find('xyz'), ('a.b'):find('.', 1, true))",
 "show(('key=value'):match('(%w+)=(%w+)'), ('  trim  '):match('^%s*(.-)%s*$'), ('abc'):match('()b()'))",
 "for k, v in ('a=1, b=2, c=3'):gmatch('(%w+)=(%w+)') do show(k, v) end",
 "show(('hello world'):gsub('o', '0'), ('abc'):gsub('%w', '%0%0'), ('abc'):gsub('', '-'), ('hello'):gsub('l', {l='L'}), ('hello'):gsub('.', function(c) if c=='e' then return false end return c:upper() end))",
 "show(pcall(string.gsub, 'abc', '(', 'x')) show(pcall(string.gsub, 'abc', '%', 'x')) show(pcall(string.gsub, 'abc', 'b', '%2'))",
 "show(string.pack('<i4', 100):byte(1,-1)) show(string.unpack('<i4', string.pack('<i4', -2))) show(string.packsize('i4i8'))",
 "show(#string.pack('z', 'hello'), string.unpack('s1', string.pack('s1', 'abc')))",
 "show(('abc'):upper(), ('ABC'):lower(), ('abc'):reverse(), ('abc'):len(), #'abc')",
 "show('10' + 5, '3' * '4', 10 .. 20, '0x10' + 0, pcall(function() return 'a' + 1 end))",
 "show(pcall(function() return {} .. 'x' end))",
 "local s = string.dump(function(a) return a + 1 end) show(#s > 0, load(s, 'x', 'b')(41))",
 "show(load(string.dump(function() return 1 end), 'x', 't'))",
 "show(tostring(12):rep(2), (12):type())",
]: S(s)
for s in [
 "local t = {1,2,3} table.insert(t, 4) table.insert(t, 1, 0) show(table.concat(t, ','))",
 "local t = {1,2,3} show(table.remove(t), table.remove(t, 1), #t, table.remove({}), table.remove({}, 0))",
 "show(table.concat({}, ','), table.concat({1,2,3}, ', ', 2, 3), table.concat({'a','b'}), pcall(table.concat, {1,{},3}))",
 "show(table.unpack({1,2,3})) show(table.unpack({1,2,3}, 2)) show(table.unpack({1,2,3}, 2, 5)) show(select('#', table.unpack({}, 1, 3)))",
 "local p = table.pack(1,nil,3) show(p.n, p[1], p[2], p[3])",
 "show(table.concat(table.move({1,2,3}, 1, 3, 2), ','), table.concat(table.move({1,2,3}, 2, 3, 1), ','), table.concat(table.move({1,2,3},1,3,1,{}), ','))",
 "local t = {5,2,8,1,9,3} table.sort(t) show(table.concat(t, ',')) table.sort(t, function(a,b) return a>b end) show(table.concat(t, ','))",
 "local t = {'b','a','c'} table.sort(t) show(table.concat(t))",
 "show(pcall(table.sort, {3,2,1,4,5,6,7,8,9,10,11,12}, function(a,b) return true end))",
 "show(pcall(table.sort, {1,'a',2}))",
 "local calls = 0 local t = {} for i=1,100 do t[i] = (i*37)%101 end table.sort(t, function(a,b) calls = calls + 1 return a<b end) show(calls, t[1], t[100])",
 "local t = setmetatable({}, {__index=function(_,i) if i<=3 then return i*10 end end, __len=function() return 3 end}) show(table.concat(t, ','), table.unpack(t))",
 "show(pcall(table.insert, {1,2,3}, 6, 1)) show(pcall(table.insert, {}, 1, 2, 3)) show(pcall(table.remove, {1,2,3}, 7))",
]: add("table_ok", s)
for s in [
 "show(math.abs(-3), math.abs(-3.5), math.abs(math.mininteger), math.ceil(3.2), math.floor(-3.2), math.ceil(2^70), math.floor(2^70))",
 "show(math.max(1, 2.5, -1), math.min(3, 1, 2), math.max(2, 2.0), pcall(math.max))",
 "show(math.sqrt(16), math.sin(0), math.cos(0), math.exp(0), math.log(8, 2), math.log(100, 10), math.log(1))",
 "show(math.fmod(7, 3), math.fmod(-7, 3), math.fmod(7, -3), math.fmod(7.5, 2), pcall(math.fmod, 1, 0)) show(math.fmod(1, 0.0))",
 "show(math.modf(3.7), math.modf(-3.7), math.modf(5), math.modf(1/0), math.modf(-1/0))",
 "show(math.tointeger(3.0), math.tointeger(3.5), math.tointeger('8'), math.tointeger('x'), math.tointeger({}), math.type(1), math.type(1.0), math.type('1'), pcall(math.type))",
 "show(math.ult(1, 2), math.ult(-1, 2), math.ult(2, -1))",
 "show(math.pi, math.huge, -math.huge, math.maxinteger, math.mininteger)",
 "math.randomseed(42) local a, b, c = math.random(), math.random(10), math.random(5, 9) show(a, b, c) math.randomseed(42) show(math.random() == a)",
 "math.randomseed(7) show(math.random(0), math.random(1, 100), math.random(3)) show(pcall(math.random, 2, 1)) show(pcall(math.random, 1, 2, 3))",
 "show(7 // 2, -7 // 2, 7 % -3, -7 % 3, 7.5 // 2, 2^10, 7 / 2, 1 // 0.0, pcall(function() return 1 // 0 end), pcall(function() return 1 % 0 end))",
 "show(3 & 5, 3 | 5, 3 ~ 5, ~0, 1 << 63, 1 << 64, -1 >> 1, 2.0 & 3, pcall(function() return 1.5 & 1 end))",
 "show(math.tointeger(2^53), 2^53 == 2^53 + 1, math.maxinteger + 1 == math.mininteger, math.maxinteger // -1)",
 "show(string.format('%.3f', math.atan(1, 2)), string.format('%.3f', math.asin(0.5)), string.format('%.3f', math.acos(0.5)), string.format('%.3f', math.tan(1)))",
]: add("math_ok", s)
for s in [
 "show(utf8.char(72, 228, 8364, 128512)) show(utf8.len('häll€')) show(utf8.len('\\xff')) show(utf8.codepoint('häl', 1, -1))",
 "for p, c in utf8.codes('hä€') do show(p, c) end",
 "show(utf8.offset('hä€x', 3), utf8.offset('hä€x', -1), utf8.offset('abc', 0, 2), pcall(utf8.offset, 'hä', 1, 3))",
 "show(utf8.charpattern == '[\\0-\\x7F\\xC2-\\xFD][\\x80-\\xBF]*', pcall(utf8.codepoint, '\\xff'))",
 "show(utf8.len('abc', 4), pcall(utf8.len, 'abc', 5), utf8.len('', 1), utf8.char())",
]: add("utf8_ok", s)
for s in [
 "show(tonumber('  10  '), tonumber('10', 2), tonumber('ff', 16), tonumber('zz', 36), tonumber('8', 8), tonumber(''), tonumber('1e2'), tonumber('0x1p4'), tonumber(nil))",
 "show(pcall(tonumber)) show(pcall(tonumber, 'x', 99)) show(pcall(tonumber, 10, 10))",
 "show(select('#'), select('#', nil, nil), select(2, 'a', 'b', 'c'), select(-1, 'a', 'b'), pcall(select, 0), pcall(select, -5, 1))",
 "show(type(nil), type(1), type('s'), type({}), type(print), type(function() end), type(coroutine), pcall(type))",
 "show(tostring(nil), tostring(true), tostring(12), tostring(1.5), tostring(-0.0), tostring(1e100), tostring(2^63), tostring(1/0), tostring(-1/0), tostring(0/0 ~= 0/0))",
 "local mt = {__tostring = function() return 'OBJ' end} show(tostring(setmetatable({}, mt))) show(pcall(tostring, setmetatable({}, {__tostring = function() return 1 end})))",
 "show(pcall(tostring, setmetatable({}, {__tostring = function() return {} end})))",
 "local t = setmetatable({}, {__metatable = 'locked'}) show(getmetatable(t), pcall(setmetatable, t, {})) show(getmetatable('x').__index == string)",
 "show(rawequal('a', 'a'), rawequal({}, {}), rawlen({1,2}), rawlen('abc'), pcall(rawlen, 5), rawget({5}, 1), rawset({}, 'a', 1).a)",
 "show(pcall(rawset, {}, nil, 1)) show(pcall(rawset, {}, 0/0, 1)) show(pcall(next, {}, 'nokey'))",
 "show(next({}), next({10}), pcall(next))",
 "local t = setmetatable({}, {__pairs = function(t) return function(_, k) if not k then return 1, 'x' end end, t, nil end}) for k, v in pairs(t) do show(k, v) end",
 "for i, v in ipairs(setmetatable({}, {__index = function(_, i) if i < 4 then return i end end})) do show(i, v) end",
 "show(pcall(error)) show(pcall(error, nil)) show(pcall(error, 'm', 0)) show(pcall(error, 'm', 1)) show(pcall(error, 'm', 2)) show(pcall(error, {code=1}))",
 "show(select('#', assert(1, 2, 3)), pcall(assert, false), pcall(assert, nil, 'msg'), pcall(assert, false, {}), pcall(assert))",
 "show(xpcall(function() error('e', 0) end, function(m) return 'H:' .. m end)) show(xpcall(function(a, b) return a + b end, print, 1, 2)) show(pcall(xpcall))",
 "show(pcall(pcall)) show(pcall(pcall, error, 'x')) show(pcall(function() local x <close> = nil; error('boom', 0) end))",
 "show(load('return 1', 'c', 't', {})()) show(load('x = 1', 'c', 't', {})) show(load('return', '=c')) show(load('error', '=c')) show(load(function() return nil end))",
 "local parts = {'return ', '1', '+2'} local i = 0 show(load(function() i = i + 1 return parts[i] end)())",
 "show(load('return _ENV', 'c', 't', 5)()) show(pcall(load, 5)) show(load('return 1', 'c', 'b')) show(load('\\27Lua', 'c', 't'))",
 "show(pcall(load, function() return 1 end)) show(load(function() error('rd') end)) show(load(function() return {} end))",
 "show(collectgarbage('count') > 0, collectgarbage('isrunning'), collectgarbage('step'), collectgarbage('collect'), pcall(collectgarbage, 'bogus'))",
 "show(_VERSION, _G == _ENV, _G._G == _G)",
 "show(pcall(dofile, '/nonexistent/file.lua')) show(loadfile('/nonexistent/file.lua'))",
 "local f = io.open(os.tmpname(), 'w') f:close() show(pcall(f.close, f))",
 "print(1, nil, true, 'x', 1.5, {} ~= nil) print() print(('x'):rep(3))",
 "print(setmetatable({}, {__name = 'My.Type'}) ~= nil, (tostring(setmetatable({}, {__name = 'My.Type'})):gsub('0x%x+', 'PTR')))",
 "print((tostring(print):gsub('0x%x+', 'PTR')), (tostring({}):gsub('0x%x+', 'PTR')), (tostring(io.stdout):gsub('0x%x+', 'PTR')))",
]: add("base_ok", s)
for s in [
 "local d = os.date('!*t', 86400) show(d.year, d.month, d.day, d.hour, d.isdst) show(os.date('!%Y-%m-%d %H:%M:%S', 3600))",
 "show(os.time{year=2020, month=1, day=1, hour=12}, os.time{year=2020, month=14, day=35, hour=0} ~= nil, pcall(os.time, {year=2020}))",
 "show(os.difftime(10, 5), pcall(os.difftime), type(os.clock()), os.getenv('RN_NOPE_XYZ'), type(os.getenv('PATH')))",
 "show(pcall(os.date, '%Ez')) show(pcall(os.date, '%Q')) show(os.date('!%c', 0)) show(os.date('!%x %X %p', 0))",
 "show(os.setlocale(), os.setlocale('C'), os.setlocale(nil, 'all'), os.setlocale('nonexistent_locale_zz'), pcall(os.setlocale, 'C', 'bogus'))",
 "show(os.remove('/nonexistent/zzz')) show(os.rename('/nonexistent/a', '/nonexistent/b'))",
 "show(os.execute() ~= nil, os.execute('exit 3')) show(os.execute('true'))",
 "local n = os.tmpname() show(type(n)) os.remove(n)",
]: add("os_ok", s)
for s in [
 "local n = os.tmpname() local f = assert(io.open(n, 'w')) show(f:write('line1\\nline2\\n', 3, ' ', 4.5, '\\n')) f:close() for l in io.lines(n) do show(l) end os.remove(n)",
 "local n = os.tmpname() local f = io.open(n, 'w') f:write('12 0x1F -3.5e2 abc\\nrest') f:close() f = io.open(n) show(f:read('n', 'n', 'n', 'n')) show(f:read('l')) show(f:read('a')) show(f:read('a'), f:read('l'), f:read(0)) f:close() os.remove(n)",
 "local n = os.tmpname() local f = io.open(n, 'w+') f:write('hello world') show(f:seek('set', 6), f:read(3), f:seek('cur'), f:seek('end'), f:seek('set')) f:close() os.remove(n)",
 "local n = os.tmpname() local f = io.open(n, 'wb') f:setvbuf('full', 1024) f:write('x') show(f:flush() == f, io.type(f), io.type(5)) f:close() show(io.type(f), tostring(f)) os.remove(n)",
 "show(io.open('/nonexistent/x', 'r')) show(pcall(io.open, 'x', 'bad')) show(pcall(io.lines, '/nonexistent/x'))",
 "local n = os.tmpname() local f = io.open(n, 'w') f:write('a\\nb\\nc') f:close() local t = {} for a, b in io.lines(n, 1, 'l') do t[#t+1] = a .. '/' .. tostring(b) end show(table.concat(t, ';')) os.remove(n)",
 "local n = os.tmpname() local f = io.open(n, 'w') f:write('x') f:close() local g = io.open(n) show(pcall(g.write, g, 'y')) show(g:read(5), g:read(1)) g:close() show(pcall(g.read, g)) os.remove(n)",
 "show(io.write('') == io.stdout, io.output() == io.stdout, io.input() == io.stdin, io.stdout:seek('cur') ~= false)",
 "local n = os.tmpname() io.output(n) io.write('via default') io.close() io.output(io.stdout) io.input(n) show(io.read('a')) io.close(io.input()) io.input(io.stdin) os.remove(n)",
 "local p = io.popen('echo hi', 'r') show(p:read('a')) show(p:close()) local q = io.popen('exit 2') show(q:close())",
 "local f = io.tmpfile() f:write('tmp') f:seek('set') show(f:read('a')) f:close()",
 "local n = os.tmpname() local f = io.open(n, 'w') f:write('1\\n2\\n') f:close() for l in io.open(n):lines('n') do show(l) end os.remove(n)",
 "show(pcall(io.read, 'x')) show(pcall(io.read, '*x')) show(pcall(io.lines, nil, {}))",
]: add("io_ok", s)
for s in [
 "local n = os.tmpname() local f = io.open(n..'.lua', 'w') f:write('return {name=..., path=select(2, ...)}') f:close() package.path = n..'.lua;' .. package.path package.preload.mymod = function(name, extra) return {name = name, extra = extra} end show(require('mymod').name, require('mymod').extra) os.remove(n..'.lua') os.remove(n)",
 "show(pcall(require, 'definitely.not.here'))",
 "show(package.searchpath('x.y', 'a/?.lua;b/?/init.lua'))",
 "show(package.searchpath('x.y', 'a/?.lua', '.', '_'))",
 "show(type(package.loaded), package.loaded._G == _G, package.loaded.string == string, type(package.preload), #package.searchers, package.config:sub(1,1))",
 "show(package.loadlib('/nonexistent.so', 'f'))",
 "local d = os.tmpname() os.remove(d) os.execute('mkdir ' .. d) local f = io.open(d .. '/m1.lua', 'w') f:write('local n = ... ; M = (M or 0) + 1; return {n = n}') f:close() package.path = d .. '/?.lua' show(require('m1').n, M, require('m1').n, M, package.loaded.m1 ~= nil) package.loaded.m1 = nil show(select(2, require('m1')) == d .. '/m1.lua', M) os.execute('rm -rf ' .. d)",
 "package.preload.p = function() return nil end show(require('p'), package.loaded.p)",
 "show(pcall(require)) show(pcall(require, {}))",
 "local d = os.tmpname() os.remove(d) os.execute('mkdir ' .. d) local f = io.open(d .. '/bad.lua', 'w') f:write('x = = 1') f:close() package.path = d .. '/?.lua' local ok, e = pcall(require, 'bad') show(ok, (e:gsub(d, 'DIR'))) os.execute('rm -rf ' .. d)",
]: add("package_ok", s)
for s in [
 "local function f(a, b) local c = 3 return debug.getinfo(1, 'nSlutf') end local i = f() show(i.name, i.namewhat, i.what, i.short_src, i.linedefined, i.lastlinedefined, i.currentline, i.nups, i.nparams, i.isvararg, i.istailcall, i.func == f)",
 "show(debug.getinfo(print).what, debug.getinfo(print, 'S').short_src, debug.getinfo(1, 'l').currentline, debug.getinfo(100), pcall(debug.getinfo, 1, 'X'), pcall(debug.getinfo, 1, '>S'))",
 "local function f(a, b) local c = 3 local n, v = debug.getlocal(1, 3) return n, v, debug.getlocal(1, 1), debug.getlocal(1, 10) end show(f(7, 8))",
 "local function f(a) debug.setlocal(1, 1, 99) return a end show(f(1)) show(debug.getlocal(f, 1))",
 "local a, b = 1, 2 local function f() return a + b end show(debug.getupvalue(f, 1)) show(debug.getupvalue(f, 2)) show(debug.getupvalue(f, 3)) show(debug.setupvalue(f, 1, 10), f()) show(debug.upvalueid(f, 1) ~= debug.upvalueid(f, 2))",
 "local a = 1 local function f() return a end local function g() return a end show(debug.upvalueid(f, 1) == debug.upvalueid(g, 1)) local x = 5 local function h() return x end debug.upvaluejoin(f, 1, h, 1) show(f())",
 "show(debug.getmetatable('').__index == string, debug.setmetatable(10, {__index = {double = function(n) return n * 2 end}}) == 10, (5):double()) debug.setmetatable(10, nil)",
 "show(type(debug.getregistry()), debug.gethook())",
 "show(pcall(debug.setuservalue, 1, 2)) show(debug.getuservalue(1)) show(debug.getuservalue(io.stdout, 1))",
 "print((debug.traceback('msg'):gsub('\\n.*', ''))) print(type(debug.traceback())) print(debug.traceback({}) ~= nil) print(debug.traceback(nil) ~= nil)",
 "local function lvl3() return debug.traceback('tb', 1) end local function lvl2() return lvl3() end local function lvl1() local r = lvl2() return r end print(lvl1())",
 "local function rec(n) if n == 0 then return debug.traceback('deep') end return (rec(n - 1)) end print(rec(30))",
 "print(select(2, pcall(function() local t = nil; return t.x end)))",
 "print(select(2, pcall(function() return #5 end)), select(2, pcall(function() return {} < {} end)), select(2, pcall(function() return nil .. 'x' end)))",
 "local function f() error('in f') end print(select(2, xpcall(f, debug.traceback)))",
 "print(pcall(string.rep, 'x', -1)) print(select(2, pcall(('x').rep, 'x', {})))",
 "local t = setmetatable({}, {__index = function(t, k) return debug.getinfo(1, 'n').name end}) show(t.foo)",
 "local co = debug.getinfo(1, 't') show(co.istailcall) local function tc() return debug.getinfo(1, 't').istailcall end local function caller() return tc() end show(caller())",
 "local lines = {} for l in pairs(debug.getinfo(1, 'L').activelines) do lines[#lines+1] = l end show(#lines)",
 "show(debug.sethook(), debug.gethook())",
 "local cnt = 0 debug.sethook(function(ev, line) cnt = cnt + 1 end, 'l') local x = 1 x = 2 x = 3 debug.sethook() show(cnt > 0, debug.gethook())",
]: add("debug_ok", s)

# chunking
n = 0
for lib, ss in snips.items():
    per = 12 if lib.endswith("_ok") else 14
    for i in range(0, len(ss), per):
        chunk = ss[i:i+per]
        n += 1
        with open(os.path.join(OUT, f"lib_{lib}_{i//per:03d}.lua"), "w") as f:
            f.write(HEAD + "\n".join(chunk) + "\n")
print("wrote", n, "scripts")
