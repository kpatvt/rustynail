local mt = {}
for _, ev in ipairs{"add","sub","mul","div","mod","pow","unm","idiv","band","bor","bxor","shl","shr","bnot","concat","len","eq","lt","le","index","newindex","call"} do
  mt["__" .. ev] = function(...) return coroutine.yield(ev) end
end
local o = setmetatable({}, mt)
local o2 = setmetatable({}, mt)
local co = coroutine.wrap(function()
  local r = {}
  r[#r+1] = o + 1  r[#r+1] = o - 1  r[#r+1] = o * 1  r[#r+1] = o / 1  r[#r+1] = o % 1
  r[#r+1] = o ^ 1  r[#r+1] = -o  r[#r+1] = o // 1  r[#r+1] = o & 1  r[#r+1] = o | 1
  r[#r+1] = o ~ 1  r[#r+1] = o << 1  r[#r+1] = o >> 1  r[#r+1] = ~o  r[#r+1] = o .. "x"
  r[#r+1] = #o  r[#r+1] = (o == o2)  r[#r+1] = (o < o2)  r[#r+1] = (o <= o2)  r[#r+1] = o.k
  o.k = 5
  r[#r+1] = o(1)
  r[#r+1] = "a" .. "b" .. o .. "c" .. "d"
  return r
end)
local v = co()
local n = 0
while type(v) == "string" do n = n + 1 io.write(v, " ") v = co(n) end
print()
print(#v, table.concat({tostring(v[1]), tostring(v[#v-1]), tostring(v[#v])}, ","))
