-- BUNDLE-T: argument / result counts and wanted-result shapes at the yield call site and the resume call site
local yield, wrap, create, resume, status = coroutine.yield, coroutine.wrap, coroutine.create, coroutine.resume, coroutine.status
local function show(...) local t = table.pack(...) local s = {} for i = 1, t.n do s[i] = tostring(t[i]) end return t.n .. ":" .. table.concat(s, ",") end

-- yield with every wanted-result count: statement, 1, 2, multi (call argument), tail position
local co = create(function(...)
  local out = {}
  yield(1)                                  -- statement: 0 results wanted
  local a = yield(2, 3)                     -- 1 wanted
  out[#out + 1] = show(a)
  local b, c, d = yield()                   -- 3 wanted
  out[#out + 1] = show(b, c, d)
  out[#out + 1] = show(yield(4))            -- multi: all results
  out[#out + 1] = show((yield(5)))          -- parenthesised: exactly 1
  local t = {yield(6)}                      -- table constructor: all
  out[#out + 1] = show(table.unpack(t))
  out[#out + 1] = show(select("#", yield(7)))
  return table.concat(out, ";"), yield(8)   -- the last call is an open call (multi)
end)
print(show(resume(co)))                     -- start
print(show(resume(co, "x")))                -- a = "x"
print(show(resume(co, "y1", "y2")))         -- a has 1 of 2... resume passes (y1, y2) to the yield 2,3 call
print(show(resume(co)))
print(show(resume(co, 10, 20, 30, 40)))
print(show(resume(co, nil, nil)))
print(show(resume(co, false)))
print(show(resume(co)))
print(show(resume(co, 1, 2, 3)))
print(show(resume(co, "final", "z")))
print(show(resume(co)), status(co))

-- return yield(...) (a tail call of a C function), with and without values
local tc = wrap(function(...) return yield(...) end)
print(show(tc(1, 2, 3)))
print(show(tc("back", nil, nil)))
local tc2 = wrap(function() return yield() end)
print(show(tc2()))
print(show(tc2()))

-- counts 0..40 in both directions
local gen = wrap(function(n) while true do local t = {} for i = 1, n do t[i] = i end n = select("#", yield(table.unpack(t, 1, n))) end end)
for n = 0, 40, 5 do
  local r = table.pack(gen(n))
  io.write(r.n, " ")
end
print()
local g2 = wrap(function(...) local n = select("#", ...) while true do n = select("#", yield(n)) end end)
for n = 0, 40, 7 do
  local a = {} for i = 1, n do a[i] = i end
  io.write(g2(table.unpack(a, 1, n)), " ")
end
print()

-- large counts: the destination / source stack must grow
local big = wrap(function(...) local n = select("#", ...) while true do local t = {yield(n, ...)} n = #t end end)
local arr = {} for i = 1, 3000 do arr[i] = i end
local r = table.pack(big(table.unpack(arr)))
print(r.n, r[1], r[2], r[3001])
r = table.pack(big(table.unpack(arr, 1, 2500)))
print(r.n, r[1], r[2], r[2501])
r = table.pack(big())
print(r.n, r[1])
local c2 = create(function() local t = {} for i = 1, 5000 do t[i] = -i end yield(table.unpack(t)) return 1 end)
local ok, a, b, z = resume(c2)
print(ok, a, b, select("#", resume(create(function() return table.unpack(arr) end))))

-- coroutine.yield used directly as a metamethod / iterator (the yielding frame's caller executes GETFIELD, ADD, ...: `finish_op` runs)
local ymt = {}
for _, ev in ipairs{"index", "add", "concat", "len", "unm", "lt", "le", "eq", "call", "newindex"} do ymt["__" .. ev] = yield end
local yo, yo2 = setmetatable({}, ymt), setmetatable({}, ymt)
local dm = wrap(function()
  local r = {}
  r[#r + 1] = tostring(yo.field)
  r[#r + 1] = tostring(yo + 1)
  r[#r + 1] = tostring(yo .. "s")
  r[#r + 1] = tostring(#yo)
  r[#r + 1] = tostring(-yo)
  r[#r + 1] = tostring(yo < yo2)
  r[#r + 1] = tostring(yo <= yo2)
  r[#r + 1] = tostring(yo == yo2)
  r[#r + 1] = tostring(yo(5))
  yo.k = 1
  for a in yield, 1, 2 do r[#r + 1] = tostring(a) break end
  return "FINAL:" .. table.concat(r, ",", 1, #r)
end)
local v = dm()
local i = 0
while not (type(v) == "string" and v:sub(1, 6) == "FINAL:") and i < 40 do
  i = i + 1
  io.write(type(v), ";")
  v = dm(i % 2 == 0 and i or tostring(i))
end
print() print(v)
-- `local x = yield()` resumed with no values must give nil (not the callee slot's old content)
local nz = wrap(function() local x = yield("z") local y, z = yield("z2") return tostring(x) .. tostring(y) .. tostring(z) end)
print(nz()) print(nz()) print(nz())
-- a coroutine with a small stack resumed with many values (it must grow), and one yielding many values to a resumer with a small stack
local sm = create(function() local t = {yield()} return #t, t[1], t[3000] end)
resume(sm)
print(resume(sm, table.unpack(arr)))
local W = create(function()
  local g = create(function() yield() local t = {} for i = 1, 5000 do t[i] = i end yield(table.unpack(t)) end)
  resume(g)
  local r = {resume(g)}
  return #r, r[2], r[5001]
end)
print(resume(W))
-- many switches with changing shapes
local acc = 0
local sh = wrap(function(a, b) while true do a, b = yield(b, a, (a or 0) + (b or 0)) end end)
local x, y = 1, 2
for i = 1, 1500 do
  local p, q, s = sh(x, y)
  x, y = (q or 0) + 1, (p or 0) + 1
  acc = acc + (s or 0)
end
print(acc, x, y)

-- nested: a coroutine resuming another through a Lua call chain
local inner = wrap(function() for i = 1, 3 do yield("in" .. i) end return "indone" end)
local outer = wrap(function()
  local function deep(n) if n == 0 then return inner() end local r = deep(n - 1) return r end
  for i = 1, 4 do yield(deep(i)) end
  return "outdone"
end)
for i = 1, 5 do print(pcall(outer)) end
