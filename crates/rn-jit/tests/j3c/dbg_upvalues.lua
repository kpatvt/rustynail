-- upvalue manipulation while compiled functions are running
local a, b, sv, uj, f = 1, 100, debug.setupvalue, debug.upvaluejoin
function f(n)
  local s = 0
  for i = 1, n do
    s = s + a
    local _ = b
    if i == 5 then sv(f, 1, 1000) end   -- upvalue 1 of f: a
    if i == 8 then uj(f, 1, f, 2) end   -- a now shares b
  end
  return s
end
P("f", f(12))
P("a", a, "b", b)
-- the callee changes the caller's upvalues (a callee of the compiled loop)
local x, y = 1, 2
local function reader(n)
  local s = 0
  for i = 1, n do s = s + x * 10 + y end
  return s
end
local function writer(k)
  if k == 1 then debug.setupvalue(reader, 1, 5) end
  if k == 2 then debug.setupvalue(reader, 2, 7) end
  if k == 3 then local u, v = debug.upvalueid(reader, 1), debug.upvalueid(reader, 2) P("ids", u ~= v) end
end
local function drive(n)
  local s = 0
  for i = 1, n do
    writer(i)
    s = s + reader(3)
  end
  return s
end
P("drive", drive(6))
-- closures made in a compiled loop: ids, join, setupvalue on them
local fs = {}
for i = 1, 20 do
  local c = i
  fs[i] = function(d) c = c + (d or 0) return c end
end
local same = 0
for i = 1, 20 do for j = 1, 20 do if debug.upvalueid(fs[i], 1) == debug.upvalueid(fs[j], 1) then same = same + 1 end end end
P("ids same", same)
for i = 1, 20, 2 do debug.upvaluejoin(fs[i], 1, fs[i + 1], 1) end
local res = {}
for i = 1, 20 do res[#res + 1] = fs[i](i) end
P(table.concat(res, ","))
res = {}
for i = 1, 20 do res[#res + 1] = fs[i](0) end
P(table.concat(res, ","))
for i = 1, 20 do debug.setupvalue(fs[i], 1, i * 3) end
res = {}
for i = 1, 20 do res[#res + 1] = fs[i](0) end
P(table.concat(res, ","))
-- running closure that has an open upvalue (the loop's own local captured by an inner function)
local function openup(n)
  local acc = 0
  local function inc() acc = acc + 1 return acc end
  for i = 1, n do
    inc()
    if i == 3 then debug.setupvalue(inc, 1, 100) end   -- changes acc (open upvalue of this frame)
    if i == 6 then local nm, v = debug.getupvalue(inc, 1) P("up", nm, v) end
    acc = acc + 10
  end
  return acc
end
P("openup", openup(8))
-- upvaluejoin that makes an open upvalue shared with a closed one
local function joiner(n)
  local live = 0
  local function bump() live = live + 1 return live end
  local dead
  do
    local z = 50
    dead = function() z = z + 1 return z end
  end
  for i = 1, n do
    bump()
    if i == 4 then debug.upvaluejoin(bump, 1, dead, 1) end
    live = live + 100
  end
  return live, dead()
end
P("joiner", joiner(7))
-- error cases
P(pcall(debug.upvaluejoin, f, 50, f, 1))
P(pcall(debug.upvalueid, f, 50))
P(debug.setupvalue(f, 9, 1), debug.getupvalue(f, 9))
P(select("#", debug.upvalueid(print, 1)))
return table.concat(out, "\n")
