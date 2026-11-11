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
print(select(2, pcall(function() local t = nil; return t.x end)))
print(select(2, pcall(function() return #5 end)), select(2, pcall(function() return {} < {} end)), select(2, pcall(function() return nil .. 'x' end)))
local function f() error('in f') end print(select(2, xpcall(f, debug.traceback)))
print(pcall(string.rep, 'x', -1)) print(select(2, pcall(('x').rep, 'x', {})))
local t = setmetatable({}, {__index = function(t, k) return debug.getinfo(1, 'n').name end}) show(t.foo)
local co = debug.getinfo(1, 't') show(co.istailcall) local function tc() return debug.getinfo(1, 't').istailcall end local function caller() return tc() end show(caller())
local lines = {} for l in pairs(debug.getinfo(1, 'L').activelines) do lines[#lines+1] = l end show(#lines)
show(debug.sethook(), debug.gethook())
local cnt = 0 debug.sethook(function(ev, line) cnt = cnt + 1 end, 'l') local x = 1 x = 2 x = 3 debug.sethook() show(cnt > 0, debug.gethook())
