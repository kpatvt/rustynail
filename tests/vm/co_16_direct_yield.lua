-- N2: the direct `coroutine.yield` of the native call path and the inlined fused resume: yield shapes (argument / result counts, wanted
-- results of the call), the cases that must stay on the library path (main thread, non-yieldable calls, hooks, stack growth, tail calls,
-- metamethods, iterators, pcall), suspended-coroutine introspection, close / tbc, errors through resume and wrap, GC of suspended coroutines
local yield, wrap, create, resume, status, running, isyieldable, close =
  coroutine.yield, coroutine.wrap, coroutine.create, coroutine.resume, coroutine.status, coroutine.running, coroutine.isyieldable, coroutine.close
local function show(...) local t = table.pack(...) local s = {} for i = 1, t.n do s[i] = tostring(t[i]) end return t.n .. ":" .. table.concat(s, ",") end

-- yield from the main thread, bare and through pcall / a Lua function
print(pcall(yield))
print(pcall(yield, 1, 2))
print(pcall(function() return yield(1) end))
print(pcall(function() local x = yield(1) return x end))
print(isyieldable(), select(2, running()))

-- wanted results of the yield call: none, one, two, three, many; argument counts 0, 1, 2, 3
local co = create(function(...)
  yield()                                      -- statement: no result wanted
  local a = yield(1)                           -- one
  local b, c = yield(1, 2)                     -- two
  local d, e, f = yield(1, 2, 3)               -- three
  local t = {yield()}                          -- many
  local u = {yield(nil, nil)}
  local g, h, i, j, k = yield(1, nil, 3)       -- more than three
  print("got", show(a), show(b, c), show(d, e, f), show(table.unpack(t, 1, #t)), #u, show(g, h, i, j, k))
  return "end", ...
end)
print(show(resume(co, "s1", "s2")))
print(show(resume(co)))
print(show(resume(co, "A")))
print(show(resume(co, "B", "C")))
print(show(resume(co, "D", "E", "F")))
print(show(resume(co, "x", "y", "z")))
print(show(resume(co)))
print(show(resume(co, 10, nil, 30, nil, 50)))
print(status(co), show(resume(co)))

-- yield in a call whose result is used in an expression / as an argument / in a multi-value position
local c2 = create(function()
  local r = 1 + yield("e1")
  print("r", r)
  print("args", yield("e2"), yield("e3"))
  local t = {yield("e4"), yield("e5")}
  print("t", #t, t[1], t[2])
  return (yield("e6"))
end)
for i = 1, 8 do print(show(resume(c2, i * 10, i * 100))) end

-- many values: more than the C stack guarantees (the call falls back to the library path when the stack must grow)
local big = {}
for i = 1, 300 do big[i] = i end
local c3 = create(function() local r = {yield(table.unpack(big))} return #r end)
local n, first, last = 0, nil, nil
do
  local ok, a, b, c = resume(c3)
  print(ok, a, b, c, select("#", resume(create(function() return table.unpack(big) end))))
end
print(show(resume(c3, table.unpack(big, 1, 200))))
print(status(c3))
local c4 = create(function(...) local n = select("#", yield(...)) return n end)
print(select("#", resume(c4, table.unpack(big, 1, 250))))
print(show(resume(c4, table.unpack(big, 1, 250))))

-- tail call of yield, yield as a method / through a local alias / through a table field / through varargs
local c5 = create(function(...) return yield(...) end)
print(show(resume(c5, 1, 2, 3))) print(show(resume(c5, "t1", "t2"))) print(status(c5))
local obj = {y = yield}
local c6 = create(function(...) local v = {obj.y(...)} return #v, obj.y(5) end)
print(show(resume(c6, "m1"))) print(show(resume(c6, "m2", "m3"))) print(show(resume(c6, "m4")))
local c7 = create(function(...) local a, b = ... return select("#", yield(a, b, ...)) end)
print(show(resume(c7, 1, nil, 3))) print(show(resume(c7, 1, 2, 3, 4)))

-- yield called from a non-yieldable context: stays an error with the reference's text
print(pcall(wrap(function() table.sort({3, 2, 1}, function(x, y) yield() return x < y end) end)))
print(pcall(wrap(function() return string.gsub("abc", "%w", function(c) yield(c) end) end)))
print(pcall(wrap(function() return load(function() yield() end) end)))
print(pcall(wrap(function() return select(2, pcall(string.rep, "x", 1, yield)) end)))
local mt = {__tostring = function() yield("in tostring") return "x" end}
print(pcall(wrap(function() return tostring(setmetatable({}, mt)) end)))
print(pcall(wrap(function() return xpcall(yield, print, 1) end)))
-- yield inside a coroutine that was resumed from inside a non-yieldable callback of another one: only the inner is yieldable
print(pcall(wrap(function()
  table.sort({2, 1}, function(a, b)
    local inner = create(function() yield("inner") return "back" end)
    print(show(resume(inner))) print(show(resume(inner)))
    return a < b
  end)
  return "sorted"
end)))

-- isyieldable in a non-yieldable callback and of a suspended coroutine; yield in a message handler
print(pcall(wrap(function() local r; table.sort({2, 1}, function(a, b) r = isyieldable() return a < b end) return r, isyieldable() end)))
local sus = create(function() yield() end)
resume(sus)
print(isyieldable(sus), isyieldable(create(function() end)), isyieldable(coroutine.running()))
print(pcall(wrap(function() return xpcall(error, function(m) yield("in handler") return m end, "boom") end)))
print(pcall(wrap(function() return select("#", pcall(error)), yield("after pcall(error)") end)))

-- yield through pcall / xpcall / error handlers / metamethods / iterators (yieldable boundaries)
local c8 = create(function()
  print("pcall", show(pcall(function() local v = yield("in pcall") error("after " .. tostring(v), 0) end)))
  print("xpcall", show(xpcall(function() yield("in xpcall") error("E", 0) end, function(m) return "handled " .. m end)))
  print("pcall(yield)", show(pcall(yield, "direct")))
  local mt2 = {
    __index = function(t, k) return yield("index " .. k) end,
    __newindex = function(t, k, v) yield("newindex " .. k) rawset(t, k, v) end,
    __add = function(a, b) return yield("add") end,
    __lt = function(a, b) return yield("lt") end,
    __le = function(a, b) return yield("le") end,
    __eq = function(a, b) return yield("eq") end,
    __concat = function(a, b) return yield("concat") end,
    __len = function(a) return yield("len") end,
    __call = function(self, x) return yield("call " .. tostring(x)) end,
    __unm = function(a) return yield("unm") end,
  }
  local o, p = setmetatable({}, mt2), setmetatable({}, mt2)
  print("mm", show(o.field), show(o + 1), show(o < p), show(o <= p), show(o == p), show(o .. "s"), show(#o), show(o(7)), show(-o))
  o.k = 5
  print("rawk", rawget(o, "k"))
  for k, v in (function() return function(_, i) i = (i or 0) + 1 if i <= 3 then return i, yield("iter " .. i) end end end)() do print("for", k, v) end
  for k in yield do print("for-yield", k) if k == "stop" then break end end
  return "c8 done"
end)
local i = 0
while true do
  i = i + 1
  local r = table.pack(resume(c8, i == 19 and "stop" or ("r" .. i)))
  print(i, show(table.unpack(r, 1, r.n)))
  if status(c8) == "dead" or i > 60 then break end
end

-- generators in generic for loops (wrap) with and without arguments
local function gen(n) return wrap(function() for i = 1, n do yield(i, i * i) end end) end
for a, b in gen(4) do print("gen", a, b) end
local acc = wrap(function(x) local s = 0 while true do s = s + x x = yield(s) if x == nil then return "fin" end end end)
print(acc(1), acc(2), acc(3), acc(nil), pcall(acc))

-- introspection of a suspended coroutine: the yield call is its top frame
local c9 = create(function(a) local lv = 5 yield(a) return lv end)
resume(c9, "arg")
local info = debug.getinfo(c9, 0, "Slnt")
print(info.what, info.short_src, info.currentline, info.name, info.namewhat, info.istailcall)
local info1 = debug.getinfo(c9, 1, "Sl")
print(info1.what, info1.currentline)
print(debug.getinfo(c9, 2, "S"))
print((debug.traceback(c9, "tb", 0)))
print((debug.traceback(c9)))
print(debug.getlocal(c9, 1, 1), debug.getlocal(c9, 1, 2))
print(status(c9), show(resume(c9)), status(c9))
print(debug.getinfo(c9, 0, "S"))

-- hooks on a coroutine thread see the yield call and the return of the library function
local function hooked(mask, count)
  local ev = {}
  local c = create(function() local x = 0 for i = 1, 2 do x = x + 1 yield(i) end return x end)
  debug.sethook(c, function(e, l)
    local inf = debug.getinfo(2, "nS")
    ev[#ev + 1] = e .. (l and (":" .. l) or "") .. ((inf and inf.what == "C") and "/C" or "")
  end, mask, count or 0)
  local r = {}
  for _ = 1, 4 do r[#r + 1] = show(resume(c)) end
  debug.sethook(c)
  return table.concat(r, " ") .. " | " .. table.concat(ev, " ")
end
print(hooked("c"))
print(hooked("r"))
print(hooked("l"))
print(hooked("", 3))
print(hooked("crl", 2))
-- a hook installed from inside the coroutine around a yield
local c10 = create(function()
  local ev = {}
  debug.sethook(function(e) ev[#ev + 1] = e end, "cr")
  yield("y1")
  yield("y2")
  debug.sethook()
  return table.concat(ev, ",")
end)
print(show(resume(c10))) print(show(resume(c10))) print(show(resume(c10)))
-- hook set by the resumer after the coroutine yielded, before it is resumed
local c11 = create(function() yield(1) local a = 1 local b = 2 yield(2) return a + b end)
resume(c11)
local ev11 = {}
debug.sethook(c11, function(e, l) ev11[#ev11 + 1] = e .. (l or "") end, "crl")
print(show(resume(c11))) print(show(resume(c11)))
debug.sethook(c11)
print(table.concat(ev11, " "))

-- close: suspended coroutine with pending to-be-closed variables; closing while a yield is pending
local function tbc(name, fail)
  return setmetatable({}, {__close = function(_, e) print("close", name, e) if fail then error("cerr " .. name, 0) end end})
end
local c12 = create(function() local a <close> = tbc("a") local b <close> = tbc("b") yield(1) return 2 end)
resume(c12)
print(show(close(c12))) print(status(c12), show(resume(c12)))
local c13 = create(function() local a <close> = tbc("fa", true) yield(1) end)
resume(c13)
print(show(close(c13))) print(status(c13))
print(show(close(create(function() end))))
local w = wrap(function() local a <close> = tbc("w") yield(1) end)
w()
-- running / normal / suspended status around nested resumes
local outer
outer = create(function()
  local inner = create(function() print("inner sees", status(outer), status(running())) yield() end)
  resume(inner)
  print(status(inner), status(outer))
  yield()
end)
resume(outer)
print(status(outer))
-- errors: after a yield, before the first yield, in wrap with positions
local e1 = create(function() yield(1) local t = nil return t.x end)
resume(e1) print(show(resume(e1))) print(status(e1), show(resume(e1)))
local ew = wrap(function() yield(1) error("wrapped") end)
ew() print(pcall(ew)) print(pcall(ew))
local ew2 = wrap(function() yield(1) error({}) end)
ew2() print(select("#", pcall(ew2)), type(select(2, pcall(function() ew2() end))))
-- an error object that is a table with __tostring through resume
print(show(resume(create(function() error(setmetatable({}, {__tostring = function() return "TS" end})) end))))
-- resume a running coroutine from itself; resume with a stack that has many values
local selfc
selfc = create(function() return resume(selfc) end)
print(show(resume(selfc)))
-- deep recursion before yielding (the Lua frames survive the switch)
local function rec(n) if n == 0 then return yield("bottom") end return 1 + rec(n - 1) end
local c14 = create(function() return rec(150) end)
print(show(resume(c14))) print(show(resume(c14, 0)))
-- many switches (the count of the benchmark shape) and values moving both ways
local acc2 = 0
local pc = create(function(a) while true do a = yield(a + 1) end end)
local v = 0
for k = 1, 2000 do local _, r = resume(pc, v) v = r end
print(v)
local wc = wrap(function(a) while true do a = yield(a * 2) end end)
local x = 1
for k = 1, 20 do x = wc(x) % 1000003 end
print(x)
-- garbage: suspended coroutines that reference tables with finalizers
collectgarbage("stop")
local log = {}
do
  local cs = {}
  for k = 1, 5 do
    local obj2 = setmetatable({k = k}, {__gc = function(o) log[#log + 1] = o.k end})
    cs[k] = create(function() local keep = obj2 yield(keep.k) yield() end)
    resume(cs[k])
  end
  cs = nil
end
collectgarbage("restart") collectgarbage() collectgarbage()
table.sort(log)
print(table.concat(log, ","))
