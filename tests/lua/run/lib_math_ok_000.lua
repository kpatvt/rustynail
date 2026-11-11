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
show(math.abs(-3), math.abs(-3.5), math.abs(math.mininteger), math.ceil(3.2), math.floor(-3.2), math.ceil(2^70), math.floor(2^70))
show(math.max(1, 2.5, -1), math.min(3, 1, 2), math.max(2, 2.0), pcall(math.max))
show(math.sqrt(16), math.sin(0), math.cos(0), math.exp(0), math.log(8, 2), math.log(100, 10), math.log(1))
show(math.fmod(7, 3), math.fmod(-7, 3), math.fmod(7, -3), math.fmod(7.5, 2), pcall(math.fmod, 1, 0)) show(math.fmod(1, 0.0))
show(math.modf(3.7), math.modf(-3.7), math.modf(5), math.modf(1/0), math.modf(-1/0))
show(math.tointeger(3.0), math.tointeger(3.5), math.tointeger('8'), math.tointeger('x'), math.tointeger({}), math.type(1), math.type(1.0), math.type('1'), pcall(math.type))
show(math.ult(1, 2), math.ult(-1, 2), math.ult(2, -1))
show(math.pi, math.huge, -math.huge, math.maxinteger, math.mininteger)
math.randomseed(42) local a, b, c = math.random(), math.random(10), math.random(5, 9) show(a, b, c) math.randomseed(42) show(math.random() == a)
math.randomseed(7) show(math.random(0), math.random(1, 100), math.random(3)) show(pcall(math.random, 2, 1)) show(pcall(math.random, 1, 2, 3))
show(7 // 2, -7 // 2, 7 % -3, -7 % 3, 7.5 // 2, 2^10, 7 / 2, 1 // 0.0, pcall(function() return 1 // 0 end), pcall(function() return 1 % 0 end))
show(3 & 5, 3 | 5, 3 ~ 5, ~0, 1 << 63, 1 << 64, -1 >> 1, 2.0 & 3, pcall(function() return 1.5 & 1 end))
