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
for i, v in ipairs(setmetatable({}, {__index = function(_, i) if i < 4 then return i end end})) do show(i, v) end
show(pcall(error)) show(pcall(error, nil)) show(pcall(error, 'm', 0)) show(pcall(error, 'm', 1)) show(pcall(error, 'm', 2)) show(pcall(error, {code=1}))
show(select('#', assert(1, 2, 3)), pcall(assert, false), pcall(assert, nil, 'msg'), pcall(assert, false, {}), pcall(assert))
show(xpcall(function() error('e', 0) end, function(m) return 'H:' .. m end)) show(xpcall(function(a, b) return a + b end, print, 1, 2)) show(pcall(xpcall))
show(pcall(pcall)) show(pcall(pcall, error, 'x')) show(pcall(function() local x <close> = nil; error('boom', 0) end))
show(load('return 1', 'c', 't', {})()) show(load('x = 1', 'c', 't', {})) show(load('return', '=c')) show(load('error', '=c')) show(load(function() return nil end))
local parts = {'return ', '1', '+2'} local i = 0 show(load(function() i = i + 1 return parts[i] end)())
show(load('return _ENV', 'c', 't', 5)()) show(pcall(load, 5)) show(load('return 1', 'c', 'b')) show(load('\27Lua', 'c', 't'))
show(load(function() error('rd') end)) show(load(function() return {} end))
show(collectgarbage('count') > 0, collectgarbage('isrunning'), collectgarbage('step'), collectgarbage('collect'), pcall(collectgarbage, 'bogus'))
show(_VERSION, _G == _ENV, _G._G == _G)
show(pcall(dofile, '/nonexistent/file.lua')) show(loadfile('/nonexistent/file.lua'))
