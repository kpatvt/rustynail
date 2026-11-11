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
show(#string.pack('z', 'hello'), string.unpack('s1', string.pack('s1', 'abc')))
show(('abc'):upper(), ('ABC'):lower(), ('abc'):reverse(), ('abc'):len(), #'abc')
show('10' + 5, '3' * '4', 10 .. 20, '0x10' + 0, pcall(function() return 'a' + 1 end))
show(pcall(function() return {} .. 'x' end))
local s = string.dump(function(a) return a + 1 end) show(#s > 0, load(s, 'x', 'b')(41))
show(load(string.dump(function() return 1 end), 'x', 't'))
show(tostring(12):rep(2), (12):type())
