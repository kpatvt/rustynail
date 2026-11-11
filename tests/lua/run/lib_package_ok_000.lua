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
local n = os.tmpname() local f = io.open(n..'.lua', 'w') f:write('return {name=..., path=select(2, ...)}') f:close() package.path = n..'.lua;' .. package.path package.preload.mymod = function(name, extra) return {name = name, extra = extra} end show(require('mymod').name, require('mymod').extra) os.remove(n..'.lua') os.remove(n)
show(pcall(require, 'definitely.not.here'))
show(package.searchpath('x.y', 'a/?.lua;b/?/init.lua'))
show(package.searchpath('x.y', 'a/?.lua', '.', '_'))
show(type(package.loaded), package.loaded._G == _G, package.loaded.string == string, type(package.preload), #package.searchers, package.config:sub(1,1))
show(package.loadlib('/nonexistent.so', 'f'))
local d = os.tmpname() os.remove(d) os.execute('mkdir ' .. d) local f = io.open(d .. '/m1.lua', 'w') f:write('local n = ... ; M = (M or 0) + 1; return {n = n}') f:close() package.path = d .. '/?.lua' show(require('m1').n, M, require('m1').n, M, package.loaded.m1 ~= nil) package.loaded.m1 = nil show(select(2, require('m1')) == d .. '/m1.lua', M) os.execute('rm -rf ' .. d)
package.preload.p = function() return nil end show(require('p'), package.loaded.p)
show(pcall(require)) show(pcall(require, {}))
local d = os.tmpname() os.remove(d) os.execute('mkdir ' .. d) local f = io.open(d .. '/bad.lua', 'w') f:write('x = = 1') f:close() package.path = d .. '/?.lua' local ok, e = pcall(require, 'bad') show(ok, (e:gsub(d, 'DIR'))) os.execute('rm -rf ' .. d)
