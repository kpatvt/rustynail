-- BUNDLE-T: error propagation, dead / non-suspended resumes, status, running, isyieldable, wrap rethrow positions
local yield, wrap, create, resume, status, running, isyieldable, close =
  coroutine.yield, coroutine.wrap, coroutine.create, coroutine.resume, coroutine.status, coroutine.running, coroutine.isyieldable, coroutine.close
local function show(...) local t = table.pack(...) local s = {} for i = 1, t.n do s[i] = tostring(t[i]) end return t.n .. ":" .. table.concat(s, ",") end

-- error after some yields; the coroutine is dead afterwards
local co = create(function(a) local b = yield(a + 1) error("boom " .. b) end)
print(show(resume(co, 1)))
print(show(resume(co, "B")))
print(status(co), show(resume(co)))
print(show(resume(co)))
-- error objects that are not strings
local e1 = create(function() yield(1) error({code = 7}) end)
resume(e1)
local ok, err = resume(e1)
print(ok, type(err), err.code)
local e2 = create(function() yield(1) error() end)
resume(e2) print(show(resume(e2)))
local e3 = create(function() yield(1) error(nil) end)
resume(e3) print(show(resume(e3)))
local e4 = create(function() yield(1) error("lvl2", 2) end)
resume(e4) print(show(resume(e4)))
local e5 = create(function() yield(1) local x = nil; return x.y end)
resume(e5) print(show(resume(e5)))
local e6 = create(function() yield(1) return 1 + {} end)
resume(e6) print(show(resume(e6)))
-- finishing normally
local f = create(function() yield(1) return "r1", "r2" end)
print(show(resume(f))) print(show(resume(f))) print(status(f)) print(show(resume(f)))
-- resume of non-suspended
local selfco
selfco = create(function() print(show(resume(selfco))) print(status(selfco)) yield() end)
print(show(resume(selfco))) print(status(selfco))
local a, b
a = create(function() print(status(a), status(b)) print(show(resume(b))) end)
b = create(function() print(status(a), status(b)) print(show(resume(a))) yield("from b") end)
print(show(resume(a)))
print(status(a), status(b))
print(show(resume(coroutine.running())))
-- wrap: dead coroutine, error rethrow with position
local w = wrap(function() yield(1) end)
w() w()
print(pcall(w))
local w2 = wrap(function() yield(1) error("werr") end)
w2()
print(pcall(w2))
local w3 = wrap(function() yield(1) error("werr", 0) end)
w3()
print(pcall(w3))
local w4 = wrap(function() yield(1) error({1, 2}) end)
w4()
local ok, e = pcall(w4) print(ok, type(e), e[2])
local w5 = wrap(function() yield(1) local t = setmetatable({}, {__index = function(t, k) error("idx " .. k) end}) return t.foo end)
w5()
print(pcall(w5))
-- error in the wrap from a nested wrap
local w6 = wrap(function() local inner = wrap(function() yield(1) error("deep") end) inner() inner() end)
print(pcall(w6))
-- running / isyieldable
print(select(2, running()), isyieldable())
print(wrap(function() local c, m = running() return type(c), m, isyieldable() end)())
local rc = create(function() return isyieldable(), select(2, running()) end)
print(resume(rc))
print(isyieldable(rc), isyieldable(create(print)))
-- yield outside a coroutine / across a C boundary
print(pcall(yield, 1))
print(pcall(wrap(function() table.sort({3, 2, 1}, function(x, y) yield() return x < y end) end)))
print(pcall(wrap(function() return string.gsub("abc", "%w", function(c) yield(c) end) end)))
print(pcall(wrap(function() local t = setmetatable({}, {__tostring = function() yield() return "x" end}) return tostring(t) end)))
-- status of a coroutine suspended in a nested resume ("normal")
local outer
outer = create(function()
  local inner = create(function() print("outer is", status(outer)) yield() end)
  resume(inner)
  print("inner is", status(inner))
end)
resume(outer)
-- type errors of the library functions
print(pcall(resume, 1))
print(pcall(resume))
print(pcall(status, nil))
print(pcall(wrap, 1))
print(pcall(create))
print(pcall(close, 1))
