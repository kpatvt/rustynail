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
local function f(a, b) local c = 3 return debug.getinfo(1, 'nSlutf') end local i = f() show(i.name, i.namewhat, i.what, i.short_src, i.linedefined, i.lastlinedefined, i.currentline, i.nups, i.nparams, i.isvararg, i.istailcall, i.func == f)
show(debug.getinfo(print).what, debug.getinfo(print, 'S').short_src, debug.getinfo(1, 'l').currentline, debug.getinfo(100), pcall(debug.getinfo, 1, 'X'), pcall(debug.getinfo, 1, '>S'))
local function f(a, b) local c = 3 local n, v = debug.getlocal(1, 3) return n, v, debug.getlocal(1, 1), debug.getlocal(1, 10) end show(f(7, 8))
local function f(a) debug.setlocal(1, 1, 99) return a end show(f(1)) show(debug.getlocal(f, 1))
local a, b = 1, 2 local function f() return a + b end show(debug.getupvalue(f, 1)) show(debug.getupvalue(f, 2)) show(debug.getupvalue(f, 3)) show(debug.setupvalue(f, 1, 10), f()) show(debug.upvalueid(f, 1) ~= debug.upvalueid(f, 2))
local a = 1 local function f() return a end local function g() return a end show(debug.upvalueid(f, 1) == debug.upvalueid(g, 1)) local x = 5 local function h() return x end debug.upvaluejoin(f, 1, h, 1) show(f())
show(debug.getmetatable('').__index == string, debug.setmetatable(10, {__index = {double = function(n) return n * 2 end}}) == 10, (5):double()) debug.setmetatable(10, nil)
show(type(debug.getregistry()), debug.gethook())
show(pcall(debug.setuservalue, 1, 2)) show(debug.getuservalue(1)) show(debug.getuservalue(io.stdout, 1))
print((debug.traceback('msg'):gsub('\n.*', ''))) print(type(debug.traceback())) print(debug.traceback({}) ~= nil) print(debug.traceback(nil) ~= nil)
local function lvl3() return debug.traceback('tb', 1) end local function lvl2() return lvl3() end local function lvl1() local r = lvl2() return r end print(lvl1())
local function rec(n) if n == 0 then return debug.traceback('deep') end return (rec(n - 1)) end print(rec(30))
