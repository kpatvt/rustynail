-- N2b: the direct `coroutine.wrap` function / `coroutine.resume` of the native call path (the fused resume inline, the interpreter entered
-- directly, results moved straight to the caller's registers, coroutines started on the fused path): result and argument shapes, errors
-- (in the coroutine, of the resume itself, through wrap), yields across pcall, close / tbc, introspection of the resumer's C frame, status
-- of nested resumers, hooks set while the coroutine runs, resume of a dead / running coroutine, GC
local yield, wrap, create, resume, status, running, isyieldable, close =
  coroutine.yield, coroutine.wrap, coroutine.create, coroutine.resume, coroutine.status, coroutine.running, coroutine.isyieldable, coroutine.close
local function show(...) local t = table.pack(...) local s = {} for i = 1, t.n do s[i] = tostring(t[i]) end return t.n .. ":" .. table.concat(s, ",") end
local main = running()

-- every wanted-result count of the call to resume / to a wrap function, for 0 .. 4 values
local function gen(...)
  local vals = table.pack(...)
  return function()
    for n = 0, vals.n do yield(table.unpack(vals, 1, n)) end
    return "done"
  end
end
do
  local co = create(gen(10, 20, 30, 40))
  resume(co)                                   -- statement
  local a = resume(co)                         -- one
  local b, c = resume(co)                      -- two
  local d, e, f = resume(co)                   -- three
  local t = {resume(co)}                       -- many
  local g, h, i, j, k = resume(co)             -- more than three (all values out)
  print(show(a), show(b, c), show(d, e, f), show(table.unpack(t)), show(g, h, i, j, k))
  print(show(resume(co)), status(co))
  local w = wrap(gen(1, 2, 3, 4))
  w()
  local a1 = w()
  local b1, c1 = w()
  local d1, e1, f1 = w()
  local t1 = {w()}
  local g1, h1, i1, j1, k1 = w()
  print(show(a1), show(b1, c1), show(d1, e1, f1), show(table.unpack(t1)), show(g1, h1, i1, j1, k1))
  print(pcall(w))
  -- results as arguments, in table constructors, returned (tail call), in arithmetic
  local w2 = wrap(function(x) while true do x = yield(x + 1, x + 2) end end)
  print(show(w2(1)), ({w2(10)})[2], select("#", w2(20)), w2(30) + w2(40))
  -- fewer values than wanted: the extra registers become nil (they held the function and the arguments)
  local w3 = wrap(function(a) local b = yield(a) local c = yield() return c end)
  local p, q = w3(5)
  local r, s2, t2 = w3(6, 7)
  local u, v = w3(8, 9)
  print(p, q, r, s2, t2, u, v)
  local co3 = create(function(a) local b = yield(a) yield() return end)
  local o1, x1, y1 = resume(co3, 5)
  local o2, x2, y2 = resume(co3, 6, 7)
  local o3, x3 = resume(co3, 8)
  print(o1, x1, y1, o2, x2, y2, o3, x3)
  local function tail() return w2(100) end
  print(show(tail()))
end

-- arguments: 0 .. 5, nil holes, many (the stack of the coroutine grows: the library path), many results back
do
  local co = create(function(...)
    local args = table.pack(...)
    while true do args = table.pack(yield(args.n, args[1], args[args.n])) end
  end)
  print(show(resume(co)))
  print(show(resume(co, 1)))
  print(show(resume(co, nil, nil)))
  print(show(resume(co, 1, nil, 3, nil, 5)))
  local big = {}
  for i = 1, 400 do big[i] = i end
  print(show(resume(co, table.unpack(big))))
  local many = wrap(function() yield(table.unpack(big)) return table.unpack(big, 1, 300) end)
  print(select("#", many()), select("#", many()))
end

-- starting coroutines (the fused start): Lua bodies with arguments, C bodies, a body that yields at once, returns at once, fails at once
do
  print(show(resume(create(function(...) return select("#", ...), ... end), "a", nil, "c")))
  print(show(wrap(string.upper)("abc")))
  print(show(wrap(select)("#", 1, 2, 3)))
  local y = wrap(yield)
  print(show(y(1, 2)), show(y(3)))
  local co = create(yield)
  print(show(resume(co, "x")), status(co), show(resume(co, "y")), status(co))
  print(show(resume(create(error), "start error")))
  local okt, et = resume(create(error), {})
  print(okt, type(et))
  print(pcall(wrap(error), "wrap start error"))
  print(pcall(wrap(error), "wrap start error", 2))
  print(show(resume(create(function() return end))))
  local w = wrap(function() return 1, 2, 3 end)
  print(show(w()), pcall(w))
  print(show(resume(create(isyieldable))), show(resume(create(function() return isyieldable(), running() == main end))))
  -- a body that is not a function (created through the C API in real programs; here: a callable table is rejected by create)
  print(pcall(create, setmetatable({}, {__call = function() end})))
end

-- errors in the resumed coroutine: strings, other values, after yields, with position, through wrap (position prefix, close of tbc)
do
  local co = create(function() yield(1) error("boom") end)
  print(show(resume(co)))
  print(show(resume(co)), status(co))
  print(show(resume(co)), status(co))
  local co2 = create(function() yield() error({code = 7}) end)
  resume(co2)
  local ok, e = resume(co2)
  print(ok, type(e), e.code, status(co2))
  local co3 = create(function() yield() error("no position", 0) end)
  resume(co3)
  print(show(resume(co3)))
  local co4 = create(function() yield() error() end)
  resume(co4)
  print(show(resume(co4)))
  local w = wrap(function() yield(1) error("in wrap") end)
  print(w())
  print(pcall(w))
  print(pcall(w))
  local w2 = wrap(function() yield(1) local t = nil; return t.x end)
  w2()
  print(pcall(w2))
  local w3 = wrap(function() yield(1) error(setmetatable({}, {__tostring = function() return "custom" end})) end)
  w3()
  local ok3, e3 = pcall(w3)
  print(ok3, tostring(e3))
  -- an error with pending to-be-closed variables: wrap closes them, resume leaves them for close()
  local w4 = wrap(function()
    local x <close> = setmetatable({}, {__close = function(_, err) print("closed x", err) end})
    yield(1)
    error("tbc error")
  end)
  w4()
  print(pcall(w4))
  local co5 = create(function()
    local y <close> = setmetatable({}, {__close = function(_, err) print("closed y", err) end})
    yield(1)
    error("tbc error 2")
  end)
  resume(co5)
  print(show(resume(co5)), status(co5))
  print(close(co5))
end

-- yields across pcall: values through, an error after the yield caught inside the coroutine (the recover path), xpcall handler
do
  local co = create(function()
    local ok, v = pcall(function() local x = yield("in pcall") return x * 2 end)
    yield(ok, v)
    local ok2, e2 = pcall(function() yield("again") error("after yield") end)
    yield(ok2, e2)
    local ok3, e3 = xpcall(function() yield("xp") error("xp error") end, function(m) return "handled: " .. m end)
    yield(ok3, e3)
    local ok4, e4 = pcall(function() yield("nested") local ok5, e5 = pcall(function() yield("inner") error("inner error") end) error("outer " .. tostring(e5)) end)
    return ok4, e4
  end)
  print(show(resume(co)))
  print(show(resume(co, 21)))
  print(show(resume(co)))
  print(show(resume(co)))
  print(show(resume(co)))
  print(show(resume(co)))
  print(show(resume(co)))
  print(show(resume(co)))
  print(show(resume(co)), status(co))
end

-- close of suspended coroutines with pending tbc (started on the fused path, suspended in a yield)
do
  local co = create(function()
    local a <close> = setmetatable({}, {__close = function() print("closed a") end})
    local b <close> = setmetatable({}, {__close = function() print("closed b") end})
    yield(1)
    print("not reached")
  end)
  print(show(resume(co)), status(co))
  print(close(co), status(co))
  print(show(resume(co)))
  local co2 = create(function()
    local c <close> = setmetatable({}, {__close = function() error("close error") end})
    yield(1)
  end)
  resume(co2)
  print(close(co2))
  local w = wrap(function() local d <close> = setmetatable({}, {__close = function() print("closed d") end}) yield(1) return 2 end)
  print(w(), w())
end

-- introspection: traceback / getinfo inside the resumed coroutine and of the resumer (its C frame for resume / the wrap function)
do
  local co = create(function()
    yield(debug.traceback("tb1"))
    yield((debug.traceback(main, "main tb"):match("^[^\n]*\n[^\n]*\n[^\n]*\n[^\n]*")))   -- (the frames below the main chunk vary)
    local i0 = debug.getinfo(main, 0, "Sn")
    local i1 = debug.getinfo(main, 1, "Sl")
    yield(i0.what, i0.short_src, i1.what, i1.currentline)
    return debug.traceback("tb end", 1)
  end)
  for _ = 1, 4 do
    local ok, a, b, c, d = resume(co)
    print(ok, a, b, c, d)
  end
  local w
  w = wrap(function()
    yield(debug.traceback("wrap tb"))
    yield((debug.traceback(main, "wrap main tb"):match("^[^\n]*\n[^\n]*\n[^\n]*\n[^\n]*")))
    local i0 = debug.getinfo(main, 0, "S")
    yield(i0.what, debug.getinfo(main, 1, "l").currentline)
  end)
  print(w())
  print(w())
  print(w())
  -- a suspended coroutine seen from outside
  local co2 = create(function() local z = 1 yield(z) end)
  resume(co2)
  print(debug.traceback(co2))
  print(debug.getinfo(co2, 0, "S").what, debug.getinfo(co2, 1, "l").currentline, debug.getlocal(co2, 1, 1))
end

-- status of nested resumers; resume of itself, of a running / normal / dead coroutine, of the main thread
do
  local A, B, C
  C = create(function()
    print("C sees", status(A), status(B), status(C), isyieldable())
    print("C resumes A", show(resume(A)))
    print("C resumes B", show(resume(B)))
    yield("c1")
    return "c end"
  end)
  B = create(function()
    print("B sees", status(A), status(B), status(C))
    print("B resumes C", show(resume(C)))
    print("B sees", status(A), status(B), status(C))
    yield("b1")
    print("B resumes C again", show(resume(C)))
    return "b end"
  end)
  A = create(function()
    print("A sees", status(A), status(B), status(C))
    print("A resumes B", show(resume(B)))
    print("A resumes itself", show(resume(A)))
    print("A resumes main", show(resume(main)))
    print("A wraps itself", pcall(wrap(function() return resume(A) end)))
    yield("a1")
    print("A resumes B again", show(resume(B)))
    return "a end"
  end)
  print(show(resume(A)), status(A), status(B), status(C))
  print(show(resume(A)), status(A), status(B), status(C))
  print(show(resume(A)), show(resume(B)), show(resume(C)))
  local dead = wrap(function() end)
  dead()
  print(pcall(dead))
  print(pcall(dead, 1, 2))
  print(show(resume(main)))
  print(pcall(resume, 1), pcall(resume))
end

-- hooks: set on the resumer by the coroutine while it runs (the results then go through the hooked post-call), on the coroutine, removed
do
  local events = {}
  local co = create(function()
    debug.sethook(main, function(e) events[#events + 1] = e .. ":" .. tostring(debug.getinfo(2, "n").name) end, "r")
    yield(1)
    debug.sethook(main)
    yield(2)
    local n = 0
    debug.sethook(function() n = n + 1 end, "", 1)
    yield(3)
    debug.sethook()
    return n > 0
  end)
  local ok1, v1 = resume(co)                   -- (fixed result counts: the hook set meanwhile must see the return of resume)
  local ok2, v2 = resume(co)
  print(ok1, v1, ok2, v2, #events, table.concat(events, " "))
  print(show(resume(co)))
  print(show(resume(co)))
  local w = wrap(function() debug.sethook(main, function(e) events[#events + 1] = "w" .. e end, "r") yield(1) debug.sethook(main) yield(2) end)
  local before = #events
  local x = w()
  local y = w()
  print(x, y, #events - before, events[#events])
end

-- metamethods and iterators calling wrap functions; a wrap function as __index / __call; many switches; GC during the run
do
  local w = wrap(function() local i = 0 while true do i = i + 1 yield(i) end end)
  local t = setmetatable({}, {__index = function(_, k) return w() end, __call = function(_, a) return w() + a end})
  print(t.x, t.y, t(100))
  local s = 0
  for v in wrap(function() for i = 1, 10 do yield(i) end end) do s = s + v end
  print(s)
  local acc = 0
  for i = 1, 3000 do acc = acc + w() end
  print(acc)
  local cos = {}
  for i = 1, 50 do
    cos[i] = wrap(function(a) local junk = {} for j = 1, 20 do junk[j] = {j} end local b = yield(a + #junk) collectgarbage("step") return b * 2 end)
  end
  local sum = 0
  for i = 1, 50 do sum = sum + cos[i](i) end
  collectgarbage()
  for i = 1, 50 do sum = sum + cos[i](i) end
  print(sum)
  local gcco = wrap(function() for i = 1, 5 do collectgarbage() yield(i) end end)
  local gs = 0
  for i = 1, 5 do gs = gs + gcco() end
  print(gs)
end
