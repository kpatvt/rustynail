-- F4: collectables as temporaries stay alive; numbers are pruned, tables and strings never are
local weak = setmetatable({}, { __mode = "v" })
local function f(n)
  local s = 0.0
  local keep = {}
  for i = 1, n do
    local len = #({ i, i + 1.5, { i } }) + #(("x"):rep(i % 5 + 1) .. i)
    s = s + (len * 1.5 + #{ i, i } * 0.5) * (i + 0.25) + (({ i * 0.5 })[1] or 0)
    if i % 9 == 0 then keep[#keep + 1] = { i * 1.5, tostring(i) .. "k" } end
    if i % 13 == 0 then weak[#weak + 1] = { i } end
    if i % 31 == 0 then collectgarbage("step", 0) end
  end
  local t = 0
  for i = 1, #keep do t = t + keep[i][1] + #keep[i][2] end
  return s, t
end
local r = 0.0
for k = 1, 20 do local a, b = f(100); r = r + a + b end
print(string.format("%.9g", r))
local a, b = f(250)
print(string.format("%.9g %.9g", a, b))
collectgarbage()
collectgarbage()
local alive = 0
for _, v in pairs(weak) do alive = alive + 1 end
print(alive)
-- an object that only a dead temporary of the frame holds (an argument slot of a finished call) stays reachable until the slot is rewritten:
-- the finalizer of the object of iteration `at` must not run in the collection of that very iteration, the one of `at - 1` must
local fins = {}
local mt = { __gc = function(o) fins[#fins + 1] = o.id end }
local function id3(a, b, c) return a end
local function probe(u, n, at)
  local s = 0.0
  local d = "-"
  for i = 1, n do
    local z = id3(i, i * 0.5, setmetatable({ id = i }, mt))
    s = s + (z * 1.5 + u[i]) * (i + 0.25)
    if i == at - 1 then
      collectgarbage()
    elseif i == at then
      local before = #fins
      collectgarbage()
      local got = {}
      for k = before + 1, #fins do got[#got + 1] = fins[k] - (at - 1) end
      table.sort(got)
      d = table.concat(got, ",")
    end
  end
  return s, d
end
local u = {}
for i = 1, 60 do u[i] = i * 0.125 end
for k = 1, 30 do probe(u, 60, 0) end
fins = {}
print(probe(u, 60, 33))
u[40] = 3 -- an integer where the trace expects a float: an exit in the middle of the iteration
print(probe(u, 60, 40))
u[41] = "2.5"
print(probe(u, 60, 41))
collectgarbage()
print(#fins > 0)
