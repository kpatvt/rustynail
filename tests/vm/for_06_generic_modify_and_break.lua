local t = {1, 2, 3, 4, 5}
for i, v in ipairs(t) do
  if v == 3 then break end
  io.write(v, " ")
end
print()
local u = {a = 1, b = 2, c = 3}
local cnt = 0
for k in pairs(u) do u[k] = nil cnt = cnt + 1 end
print(cnt, next(u))
local w = {1, 2, 3}
for i, v in ipairs(w) do if i < 6 then w[#w + 1] = v end end
print(#w)
local x = {}
for i = 1, 10 do x[i] = i end
for k, v in pairs(x) do x[k] = v * 2 end
print(x[1], x[5], x[10])
local r = {}
for i, v in ipairs({"a", "b", "c"}) do r[#r + 1] = v .. i end
print(r[1], r[2], r[3])
