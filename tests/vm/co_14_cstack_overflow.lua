-- BUNDLE-T: C stack accounting in chains of nested resumes, stack overflow inside a coroutine, GC of suspended coroutines
local yield, wrap, create, resume = coroutine.yield, coroutine.wrap, coroutine.create, coroutine.resume

-- a chain of nested resumes: the C stack limit
local depth = 0
local function nest(n)
  depth = n
  local co = create(function() return nest(n + 1) end)
  local ok, e = resume(co)
  if not ok then error(e, 0) end
  return e
end
local ok, e = pcall(nest, 1)
print(ok, e, depth > 100, depth < 250)
-- the same through wrap
local d2 = 0
local function nestw(n) d2 = n return wrap(function() return nestw(n + 1) end)() end
local ok2, e2 = pcall(nestw, 1)
print(ok2, (tostring(e2):gsub("^.-:%d+: ", ""):sub(1, 60)), d2 == depth)
-- yields do not accumulate the count: a long-running generator inside a deep (but legal) chain
local function chain(n)
  if n == 0 then
    local g = wrap(function() for i = 1, 50 do yield(i) end end)
    local s = 0 for v in g do s = s + v end
    return s
  end
  return wrap(function() return chain(n - 1) end)()
end
print(pcall(chain, 150))
print(pcall(chain, 190))
-- after the failure the counters are back to normal
print(pcall(chain, 150))
print(select(2, pcall(nest, 1)) == e)

-- a coroutine suspended at a shallow level, resumed from deep inside a chain of nested resumes: the C level of the
-- resumer counts (the number of further levels the coroutine can nest is smaller)
local probe = create(function()
  while true do
    yield()
    local n = 0
    local function dive() n = n + 1 local c = create(dive) local ok, e = resume(c) if not ok then error(e, 0) end end
    pcall(dive)
    yield(n)
  end
end)
resume(probe)
local function measure(depth)
  if depth == 0 then
    local _, n = resume(probe)
    resume(probe)
    return n
  end
  return wrap(function() return measure(depth - 1) end)()
end
local m0, m50, m120 = measure(0), measure(50), measure(120)
print(m0 - m50, m0 - m120, m0 > 150, m0 - m120 > 100)

-- stack overflow inside a coroutine
local so = create(function() local function f(n) return 1 + f(n + 1) end return f(1) end)
local okso, eso = resume(so)
print(okso, (tostring(eso):gsub("^.-:%d+: ", ""):sub(1, 30)))
print(coroutine.status(so))
local so2 = wrap(function() local t = {} local function f() t[#t + 1] = 1 yield(#t) return f() + 1 end return f() end)
print(pcall(so2)) print(pcall(so2))
-- deep recursion that yields at the bottom and the top
local dy = wrap(function() local function f(n) if n == 0 then yield("bottom") return 0 end local r = f(n - 1) return r + 1 end yield("top") return f(150) end)
print(dy(), dy(), dy())
-- error through many coroutine levels
local function lvl(n) if n == 0 then error("deepest") end local ok, e = resume(create(lvl), n - 1) if not ok then error(e, 0) end end
print(pcall(lvl, 20))

-- GC of suspended coroutines and objects reachable only from them
collectgarbage("stop")
local log = {}
local function mk(i)
  local obj = setmetatable({i = i}, {__gc = function(o) log[#log + 1] = o.i end})
  local co = create(function() local keep = obj yield(i) yield(keep.i) end)
  resume(co)
  return co
end
local keep = mk(1)
for i = 2, 6 do mk(i) end
collectgarbage("restart")
collectgarbage() collectgarbage()
table.sort(log)
print(table.concat(log, ","))
print(select(2, resume(keep)))
local weak = setmetatable({}, {__mode = "v"})
do local c = create(function() yield() end) resume(c) weak[1] = c end
collectgarbage() collectgarbage()
print(weak[1])
-- a coroutine that is garbage while running other coroutines (wrap closures dropped mid-iteration)
local sum = 0
for i = 1, 300 do
  local g = wrap(function(a) local b = yield(a * 2) return a + b end)
  sum = sum + g(i) + g(1)
end
print(sum, collectgarbage("count") > 0)
