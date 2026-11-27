-- Section 77.14 (`t1g` bit 2): generic `for` over Lua closure iterators, pushed as flat frames by tier 1 instead of through a C-level call.
-- Results nil-filled and cut to the loop's variables, iterators with extra parameters, varargs, errors with levels and tracebacks, hooks,
-- coroutine yields from inside the iterator, debug.getinfo of the iterator's caller, deep recursion inside the iterator (stack growth),
-- callable tables and C functions as iterators, nested loops and a pipeline of closures.

local function range(n, step)
  local i = 0
  return function()
    i = i + (step or 1)
    if i <= n then return i end
  end
end

local function pairs2(t)
  local k = nil
  return function()
    local v
    k, v = next(t, k)
    if k ~= nil then return k, v, k end -- one more result than the loop takes
  end
end

-- (1) plain loops, fewer / more results than variables
local function plain()
  local s = 0
  for i in range(300) do s = s + i end
  local s2, cnt = 0, 0
  for k, v, extra, missing in pairs2({ 1, 2, 3, x = 4 }) do
    cnt = cnt + 1
    s2 = s2 + v + (missing == nil and 1 or 100)
  end
  return s, s2, cnt
end
for r = 1, 40 do local a, b, c = plain(); if r % 20 == 0 then print("plain", a, b, c) end end

-- (2) an iterator with three parameters (the third is nil-filled), a vararg iterator
local function three(s, c, z) if c < 5 then return c + 1, z end end
local function va(...) local s, c = ... if c < 4 then return c + 1, select("#", ...) end end
local function params()
  local a, b = 0, 0
  for i, z in three, nil, 0 do a = a + i + (z == nil and 1 or 0) end
  for i, n in va, nil, 0 do b = b + i * 10 + n end
  return a, b
end
for r = 1, 40 do local a, b = params(); if r % 20 == 0 then print("params", a, b) end end

-- (3) errors raised inside the iterator, at level 1 and level 2, and a traceback
local function bad(lim)
  local i = 0
  return function()
    i = i + 1
    if i == lim then error("iter fail " .. i, 2) end
    if i == lim + 100 then error("never") end
    return i
  end
end
local function erring(lim)
  local s = 0
  for i in bad(lim) do s = s + i end
  return s
end
for r = 1, 30 do
  local ok, e = pcall(erring, 20 + r % 3)
  if r % 10 == 0 then print("err", ok, (tostring(e):gsub("^.-:(%d+):", "line %1:"))) end
end
local ok, tb = xpcall(erring, debug.traceback, 5)
local n = 0
for line in tb:gmatch("[^\n]+") do n = n + 1 end
print("traceback", ok, n > 3, tb:match("in function 'erring'") ~= nil or tb:match("erring") ~= nil)

-- (4) debug.getinfo from inside the iterator: the caller is the loop's function, at the line of the `for`
local seen = {}
local function info_iter()
  local i = 0
  return function()
    i = i + 1
    if i > 3 then return nil end
    local d = debug.getinfo(2, "Sln")
    seen[#seen + 1] = d.currentline .. ":" .. tostring(d.what)
    return i
  end
end
local function infos() for i in info_iter() do end end
for r = 1, 3 do infos() end
print("getinfo", seen[1], seen[#seen], #seen)

-- (5) coroutine yields from inside the iterator
local function yielding()
  local i = 0
  return function()
    i = i + 1
    if i > 5 then return nil end
    coroutine.yield(i)
    return i * 2
  end
end
local function co_loop()
  local s = 0
  for v in yielding() do s = s + v end
  return s
end
for r = 1, 20 do
  local co = coroutine.create(co_loop)
  local got = {}
  while coroutine.status(co) ~= "dead" do
    local ok, v = coroutine.resume(co)
    got[#got + 1] = tostring(v)
  end
  if r % 10 == 0 then print("yield", table.concat(got, ",")) end
end

-- (6) hooks installed while iterating (count and call hooks)
local calls = 0
local function hooked()
  local s = 0
  for i in range(200) do
    s = s + i
    if i == 50 then debug.sethook(function() calls = calls + 1 end, "c") end
    if i == 150 then debug.sethook() end
  end
  return s
end
for r = 1, 5 do print("hook", hooked(), calls > 0) end

-- (7) deep recursion inside the iterator (the stack grows and moves), callable tables and C functions as iterators
local function deep(n) if n == 0 then return 0 end return 1 + deep(n - 1) end
local function deep_iter()
  local i = 0
  return function()
    i = i + 1
    if i > 4 then return nil end
    return deep(150 * i)
  end
end
local callable = setmetatable({}, { __call = function(self, s, c) if c < 3 then return c + 1 end end })
local function misc()
  local a, b, c = 0, 0, 0
  for d in deep_iter() do a = a + d end
  for i in callable, nil, 0 do b = b + i end
  for k, v in next, { 5, 6, 7 } do c = c + v end
  return a, b, c
end
for r = 1, 30 do local a, b, c = misc(); if r % 15 == 0 then print("misc", a, b, c) end end

-- (8) a pipeline of closures folded by a generic `for` (the stream_closures shape)
local function map(f, src) return function() local v = src() if v ~= nil then return f(v) end end end
local function filter(p, src) return function() while true do local v = src() if v == nil then return nil end if p(v) then return v end end end end
local function fold(f, acc, src) for v in src do acc = f(acc, v) end return acc end
for r = 1, 30 do
  local total = fold(function(a, b) return a + b end, 0,
    map(function(x) return x * 3 end, filter(function(x) return x % 4 ~= 0 end, range(500))))
  if r % 15 == 0 then print("pipeline", total) end
end

-- (9) a function-entry trace of an iterator (loop-free, it calls its source) whose side traces return into the generic `for` that drives it
-- (`retf` bit 8), and a loop-free caller between them (`retf` bit 9)
local function src_gen(n) local i = 0 return function() i = i + 1 if i <= n then return i end end end
local function branchy(src)
  return function()
    local v = src()
    if v == nil then return nil end
    if v % 3 == 0 then return v * 2 end
    if v % 5 == 0 then return v - 1 end
    return v + 1
  end
end
local function wrapit(it) return function() local v = it() return v end end
local function consume(it) local s = 0 for v in it do s = s + v end return s end
for r = 1, 80 do
  local s = consume(branchy(src_gen(200)))
  local t = consume(wrapit(branchy(src_gen(150))))
  if r % 40 == 0 then print("retf-tfor", s, t) end
end
