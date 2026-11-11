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
show(tonumber('  10  '), tonumber('10', 2), tonumber('ff', 16), tonumber('zz', 36), tonumber('8', 8), tonumber(''), tonumber('1e2'), tonumber('0x1p4'), tonumber(nil))
show(pcall(tonumber)) show(pcall(tonumber, 'x', 99)) show(pcall(tonumber, 10, 10))
show(select('#'), select('#', nil, nil), select(2, 'a', 'b', 'c'), select(-1, 'a', 'b'), pcall(select, 0), pcall(select, -5, 1))
show(type(nil), type(1), type('s'), type({}), type(print), type(function() end), type(coroutine), pcall(type))
show(tostring(nil), tostring(true), tostring(12), tostring(1.5), tostring(-0.0), tostring(1e100), tostring(2^63), tostring(1/0), tostring(-1/0), tostring(0/0 ~= 0/0))
local mt = {__tostring = function() return 'OBJ' end} show(tostring(setmetatable({}, mt))) show(pcall(tostring, setmetatable({}, {__tostring = function() return 1 end})))
show(pcall(tostring, setmetatable({}, {__tostring = function() return {} end})))
local t = setmetatable({}, {__metatable = 'locked'}) show(getmetatable(t), pcall(setmetatable, t, {})) show(getmetatable('x').__index == string)
show(rawequal('a', 'a'), rawequal({}, {}), rawlen({1,2}), rawlen('abc'), pcall(rawlen, 5), rawget({5}, 1), rawset({}, 'a', 1).a)
show(pcall(rawset, {}, nil, 1)) show(pcall(rawset, {}, 0/0, 1)) show(pcall(next, {}, 'nokey'))
show(next({}), next({10}), pcall(next))
local t = setmetatable({}, {__pairs = function(t) return function(_, k) if not k then return 1, 'x' end end, t, nil end}) for k, v in pairs(t) do show(k, v) end
