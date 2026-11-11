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
local f = io.open(os.tmpname(), 'w') f:close() show(pcall(f.close, f))
print(1, nil, true, 'x', 1.5, {} ~= nil) print() print(('x'):rep(3))
print(setmetatable({}, {__name = 'My.Type'}) ~= nil, (tostring(setmetatable({}, {__name = 'My.Type'})):gsub('0x%x+', 'PTR')))
print((tostring(print):gsub('0x%x+', 'PTR')), (tostring({}):gsub('0x%x+', 'PTR')), (tostring(io.stdout):gsub('0x%x+', 'PTR')))
