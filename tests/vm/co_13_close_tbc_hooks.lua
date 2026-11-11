-- BUNDLE-T: close with pending to-be-closed variables, hooks inside coroutines, tracebacks of suspended coroutines
local yield, wrap, create, resume, status, close = coroutine.yield, coroutine.wrap, coroutine.create, coroutine.resume, coroutine.status, coroutine.close
local function show(...) local t = table.pack(...) local s = {} for i = 1, t.n do s[i] = tostring(t[i]) end return t.n .. ":" .. table.concat(s, ",") end
local function tbc(name, fail) return setmetatable({}, {__close = function(_, e) print("close", name, e) if fail then error("cerr " .. name, 0) end end}) end

-- close a suspended coroutine with tbc variables
local co = create(function() local a <close> = tbc("a") local b <close> = tbc("b") yield(1) yield(2) end)
print(show(resume(co)))
print(show(resume(co)))
print(show(close(co)), status(co))
-- the fused path must not skip the variables when the body finishes normally
local c2 = create(function() local a <close> = tbc("n1") yield(1) local b <close> = tbc("n2") yield(2) return "end" end)
print(show(resume(c2))) print(show(resume(c2))) print(show(resume(c2)), status(c2))
-- errors in __close
local c3 = create(function() local a <close> = tbc("e1", true) local b <close> = tbc("e2", true) yield(1) end)
resume(c3)
print(show(close(c3)))
local c4 = create(function() local a <close> = tbc("f1") yield(1) error("body") end)
resume(c4) print(show(resume(c4))) print(show(close(c4)))
-- yield inside __close
local c5 = create(function() local a <close> = setmetatable({}, {__close = function() yield("in close") print("after yield in close") end}) yield("first") return "done5" end)
print(show(resume(c5))) print(show(resume(c5))) print(show(resume(c5))) print(status(c5))
-- close inside the coroutine itself
local c6 = create(function() local a <close> = tbc("self") print(pcall(close)) yield(1) end)
print(show(resume(c6)))
-- wrap + generic for with a closing value
local function gen() return wrap(function() for i = 1, 3 do yield(i) end end), nil, nil, tbc("forclose") end
for v in gen() do io.write(v, " ") end print()
for v in gen() do io.write(v, " ") if v == 2 then break end end print()

-- hooks inside a coroutine (debug.sethook on a thread): call / line / count / return
local function hook_events(mask, count)
  local ev = {}
  local co = create(function(n)
    local s = 0
    for i = 1, n do s = s + i yield(s) end
    return s
  end)
  debug.sethook(co, function(e, l) ev[#ev + 1] = e .. (l and (":" .. l) or "") end, mask, count or 0)
  local out = {}
  for i = 1, 4 do out[#out + 1] = show(select(2, resume(co, 3))) end
  debug.sethook(co)
  return table.concat(ev, " "), table.concat(out, "|")
end
print(hook_events("c"))
print(hook_events("r"))
print(hook_events("l"))
print(hook_events("", 7))
print((hook_events("crl", 5)))
-- a hook installed on the resumer only
local ev = 0
debug.sethook(function() ev = ev + 1 end, "c")
local hw = wrap(function() for i = 1, 5 do yield(i) end end)
for i = 1, 5 do hw() end
debug.sethook()
print("resumer calls", ev > 0)
-- the hook set from inside the coroutine
local hco = create(function()
  local n = 0
  debug.sethook(function(e) n = n + 1 end, "l")
  yield(1)
  local x = 1
  x = x + 1
  yield(2)
  debug.sethook()
  return n > 0
end)
print(show(resume(hco))) print(show(resume(hco))) print(show(resume(hco)))
-- a hook that yields (count hook) : not allowed from a Lua hook ("attempt to yield across a C-call boundary")
local yh = create(function() debug.sethook(function() yield() end, "", 1) for i = 1, 3 do local x = i end end)
print(show(resume(yh)))
debug.sethook()
-- tracebacks
local t1 = create(function() local function f() yield() end f() end)
resume(t1)
print((debug.traceback(t1)))
print((debug.traceback(t1, "msg")))
print((debug.traceback(t1, nil, 1)))
print(debug.traceback(create(print)))
local t2 = create(function() error("x") end)
resume(t2)
print((debug.traceback(t2)))
print(debug.getinfo(t1, 1, "l").currentline, debug.getinfo(t1, 0, "n").name, debug.getinfo(t1, 5))
print(debug.getlocal(t1, 1, 1))
