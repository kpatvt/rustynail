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
local t = {1,2,3} table.insert(t, 4) table.insert(t, 1, 0) show(table.concat(t, ','))
local t = {1,2,3} show(table.remove(t), table.remove(t, 1), #t, table.remove({}), table.remove({}, 0))
show(table.concat({}, ','), table.concat({1,2,3}, ', ', 2, 3), table.concat({'a','b'}), pcall(table.concat, {1,{},3}))
show(table.unpack({1,2,3})) show(table.unpack({1,2,3}, 2)) show(table.unpack({1,2,3}, 2, 5)) show(select('#', table.unpack({}, 1, 3)))
local p = table.pack(1,nil,3) show(p.n, p[1], p[2], p[3])
show(table.concat(table.move({1,2,3}, 1, 3, 2), ','), table.concat(table.move({1,2,3}, 2, 3, 1), ','), table.concat(table.move({1,2,3},1,3,1,{}), ','))
local t = {5,2,8,1,9,3} table.sort(t) show(table.concat(t, ',')) table.sort(t, function(a,b) return a>b end) show(table.concat(t, ','))
local t = {'b','a','c'} table.sort(t) show(table.concat(t))
show(pcall(table.sort, {3,2,1,4,5,6,7,8,9,10,11,12}, function(a,b) return true end))
show(pcall(table.sort, {1,'a',2}))
local calls = 0 local t = {} for i=1,100 do t[i] = (i*37)%101 end table.sort(t, function(a,b) calls = calls + 1 return a<b end) show(calls, t[1], t[100])
local t = setmetatable({}, {__index=function(_,i) if i<=3 then return i*10 end end, __len=function() return 3 end}) show(table.concat(t, ','), table.unpack(t))
