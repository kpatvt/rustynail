-- JIT differential test program generator.
--   rnlua tests/jitdiff/gen.lua SEED [hook] > prog.lua
-- hook: omitted | count | line | both  (adds a debug.sethook prelude that logs event counts)
-- Output is a deterministic, side-effect free (except print) Lua 5.5 program.
-- The generator has its own PRNG (xorshift64*), independent of math.random.

local seed = math.tointeger(tonumber(arg and arg[1])) or 1
local hookmode = arg and arg[2]
if hookmode == "" or hookmode == "none" then hookmode = nil end
-- arg[4]: template set; "tr" (P8b) mixes the prototype-shaped trace templates in; omitted = the original set
-- (the stream is then exactly the pre-P8b one: old seeds reproduce)
local tset = arg and arg[4]
if tset == "" or tset == "none" then tset = nil end
-- arg[3]: comma separated snippet indices to keep (for minimisation); all snippets are still generated
-- so the PRNG stream and the text of the kept ones is unchanged.
local keep
if arg and arg[3] and arg[3] ~= "" then
  keep = {}
  for n in arg[3]:gmatch("%d+") do keep[tonumber(n)] = true end
end

local state = (seed * 0x9E3779B97F4A7C15) ~ 0x2545F4914F6CDD1D
if state == 0 then state = 88172645463325252 end
local function next64()
  state = state ~ (state << 13)
  state = state ~ (state >> 7)
  state = state ~ (state << 17)
  return state * 0x2545F4914F6CDD1D
end
for _ = 1, 8 do next64() end

-- rnd(n): 1..n ; rnd(a,b): a..b
local function rnd(a, b)
  if not b then a, b = 1, a end
  local r = (next64() >> 11) % (b - a + 1)
  return a + r
end
local function chance(p) return rnd(100) <= p end
local function pick(t) return t[rnd(#t)] end

-- loop count scale: hook mode runs slower, so use fewer iterations
local SCALE = hookmode and 5 or 1
local function N(lo, hi) return math.max(20, rnd(lo, hi) // SCALE) end

local function tpl(s, p)
  return (s:gsub("%$([%a_][%w_]*)", function(k)
    local v = p[k]
    if v == nil then error("missing template param " .. k) end
    return tostring(v)
  end))
end

------------------------------------------------------------------ expressions
local INTC = { "0", "1", "2", "3", "7", "63", "64", "255", "(-1)", "(-2)", "(-7)",
  "math.maxinteger", "math.mininteger", "(1<<62)", "0x7fffffff", "(-0x80000000)" }
local FLTC = { "0.0", "(-0.0)", "0.5", "1.5", "(-2.5)", "1e308", "(-1e308)", "(0/0)", "math.huge",
  "(-math.huge)", "2^53", "2^63", "(-2^63)", "1e15", "3.0", "(-3.0)", "1e-310" }
local ODDC = { "'10'", "'0x10'", "'abc'", "' 5 '", "nil", "true", "{}", "'1e2'" }

local function atom(vars, kind)
  local r = rnd(100)
  if r <= 45 then return pick(vars)
  elseif r <= 65 then return tostring(rnd(0, 20))
  elseif r <= 85 or kind == "int" then
    if kind == "int" or chance(50) then return pick(INTC) end
    return pick(FLTC)
  else
    if kind == "num" then return pick(FLTC) end
    return pick(chance(70) and FLTC or ODDC)
  end
end

local IOPS = { "+", "-", "*", "//", "%", "&", "|", "~", "<<", ">>", "+", "-", "*" }
local NOPS = { "+", "-", "*", "/", "//", "%", "^", "+", "-", "*", "/" }
local AOPS = { "+", "-", "*", "/", "//", "%", "^", "&", "|", "~", "<<", ">>", "..", "+", "-" }
local CMPS = { "<", "<=", ">", ">=", "==", "~=" }

local function expr(vars, kind, d)
  if d <= 0 or chance(20) then return atom(vars, kind) end
  local r = rnd(100)
  if r <= 8 then
    local u = pick({ "-", "~", "not " })
    if kind == "num" and u == "~" then u = "-" end
    return "(" .. u .. "(" .. expr(vars, kind, d - 1) .. "))"
  elseif r <= 16 then
    return "(" .. expr(vars, kind, d - 1) .. " " .. pick(CMPS) .. " " .. expr(vars, kind, d - 1) ..
        " and " .. expr(vars, kind, d - 1) .. " or " .. expr(vars, kind, d - 1) .. ")"
  elseif r <= 20 then
    return "(" .. expr(vars, kind, d - 1) .. " " .. pick({ "and", "or" }) .. " " .. expr(vars, kind, d - 1) .. ")"
  end
  local ops = kind == "int" and IOPS or kind == "num" and NOPS or AOPS
  return "(" .. expr(vars, kind, d - 1) .. " " .. pick(ops) .. " " .. expr(vars, kind, d - 1) .. ")"
end

------------------------------------------------------------------ templates
local T = {}
local uid = 0
local function newid() uid = uid + 1; return uid end

-- Per-iteration pcall of generated expressions; results folded into acc.
T[#T + 1] = function(id)
  local kind = pick({ "int", "int", "num", "any" })
  return tpl([[
local function e1_$id(i, a, b) return $E1 end
local function e2_$id(i, a, b) return $E2 end
local function s$id(x, y)
  local acc, f = x, 0.0
  for i = 1, $N do
    local ok, v = pcall(e1_$id, i, acc, y)
    if ok then
      if math.type(v) == "integer" then acc = v else f = f + 0; mix(v) end
    else
      err(v)
    end
    local ok2, w = pcall(e2_$id, i, y, acc)
    if ok2 then if math.type(w) == "integer" then y = w ~ i else mix(w) end else err(w) end
  end
  return acc, y, f
end
run("s$id", s$id, $X, $Y)
run("s$id", s$id, $Y, $X)
]], { id = id, E1 = expr({ "i", "a", "b" }, kind, 3), E2 = expr({ "i", "a", "b", "a" }, kind, 3),
    N = N(300, 3000), X = pick({ "1", "7", "math.maxinteger", "math.mininteger", "-3", "12345" }),
    Y = pick({ "0", "2", "5", "(-1)", "1<<40", "9" }) })
end

-- Float expression loops with accumulators
T[#T + 1] = function(id)
  return tpl([[
local function s$id(x)
  local a, b = x, 1.5
  for i = 1, $N do
    a = a + (i % 7) * 0.25 - b / (i + 1)
    b = (b * 1.0001 + a * 1e-6) % 100.0
    if i % $M == 0 then a = a // 3.0; mix(a) end
    if a ~= a or a == math.huge then a = i end
  end
  return a, b, math.floor(a) , a // 1, math.tointeger(b // 1)
end
run("s$id", s$id, $X)
run("s$id", s$id, 0.1)
]], { id = id, N = N(500, 4000), M = rnd(3, 50), X = pick({ "0.0", "1.0", "-0.0", "3", "1e10" }) })
end

-- Numeric for loop edge cases
local HEADERS = { "1, 10", "10, 1, -1", "1, 10, 3", "math.maxinteger - 5, math.maxinteger",
  "math.mininteger, math.mininteger + 5", "math.maxinteger - 5, math.maxinteger, 2",
  "1, math.maxinteger, math.maxinteger // 3", "math.mininteger, -1, math.maxinteger // 2",
  "1.0, 5", "1, 5.5", "0.1, 1, 0.1", "1, 0", "1, 3, 0", "1, 'x'", "1.5, 4, 1", "3, 1",
  "1, math.huge", "-1, -math.huge, -1", "0, 1, 0.25", "1e15, 1e15 + 3", "2^53, 2^53 + 4",
  "math.maxinteger - 2, 1e100", "1, 3, math.mininteger", "math.maxinteger, math.mininteger, -math.maxinteger",
  "1, 0/0", "'10', 1", "5, 1, -2", "-5, -1", "0, -0.0", "math.mininteger, math.maxinteger, math.maxinteger",
  "1, 100, 7", "100, 1, -7", "1.0, 3.0, 0.5", "0, 2^63", "math.maxinteger + 0.0, math.maxinteger + 0.0",
  "-3, 3", "1, 20, 0.5", "math.maxinteger - 1, math.maxinteger + 1.0", "nil, 3", "1, nil" }
T[#T + 1] = function(id)
  local parts = {}
  for k = 1, 3 do
    local h = pick(HEADERS)
    parts[#parts + 1] = tpl([[
  do
    local c = 0
    local ok, e = pcall(function()
      for i = $H do
        c = c + 1; mix(i)
        if c >= $CAP then break end
      end
    end)
    if not ok then err(e) end
    mix(c)
  end
]], { H = h, CAP = N(30, 400) })
  end
  return tpl([[
local function s$id()
  for rep = 1, $R do
$BODY
  end
  return rep
end
run("s$id", s$id)
]], { id = id, R = N(20, 150), BODY = table.concat(parts) })
end

-- while / repeat / goto / nested loops
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local i, s, t = 0, 0, 0
  while i < n do
    i = i + 1
    if i % $A == 0 then goto continue end
    s = s + i * $B
    for j = 1, $J do
      if (i + j) % $C == 0 then break end
      t = t + j ~ i
    end
    ::continue::
  end
  local k = 0
  repeat
    local z = k * 2
    k = k + 1
    if z > $LIM then break end
  until z + k > $LIM2
  do
    local q = 0
    ::top::
    q = q + 1
    if q < $Q then goto top end
    s = s + q
  end
  return s, t, k
end
run("s$id", s$id, $N)
run("s$id", s$id, $N2)
]], { id = id, A = rnd(2, 9), B = rnd(1, 99), J = rnd(3, 12), C = rnd(3, 11), LIM = N(50, 3000), LIM2 = N(60, 4000),
    Q = N(10, 2000), N = N(200, 2000), N2 = N(5, 200) })
end

-- Tables: array / hash / mixed keys / insert / remove / sort / concat
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local t, h = {}, {}
  for i = 1, n do t[i] = i * $M end
  for i = 1, n do local k = "k" .. (i % $K); h[k] = (h[k] or 0) + i end
  local s = 0
  for i = 1, #t do s = s + t[i] end
  mix(s)
  local m = {}
  m[1.0] = "a"; m[2^53] = "b"; m[-0.0] = "z"; m[3] = "c"; m[3.5] = "d"; m[true] = "e"
  m[1] = m[1] .. "x"
  mix(m[1]); mix(m[2^53]); mix(m[0]); mix(m[0.0]); mix(m[3.0]); mix(m[3.5]); mix(m[true]); mix(#m)
  local ok, e = pcall(function() m[0/0] = 1 end); err(e)
  ok, e = pcall(function() m[nil] = 1 end); err(e)
  ok, e = pcall(function() local u; return u.x end); err(e)
  ok, e = pcall(function() local u = {}; return u.x.y end); err(e)
  ok, e = pcall(function() return m.nope.y end); err(e)
  mix(m[0/0])
  local ks = {}
  for k in pairs(h) do ks[#ks + 1] = k end
  table.sort(ks)
  for _, k in ipairs(ks) do mix(k); mix(h[k]) end
  for i = 1, n // 3 do table.insert(t, $INS) end
  for i = 1, n // 5 do mix(table.remove(t)) end
  table.insert(t, 1, -5); mix(table.remove(t, 1)); mix(#t)
  local sq = {}
  for i = 1, n // 4 do sq[i] = (i * $P) % $Q end
  table.sort(sq, function(a, b) return a > b end)
  for i = 1, #sq, 7 do mix(sq[i]) end
  local c = {}
  for i = 1, 30 do c[i] = i * 1.5 end
  mix(table.concat(c, ","))
  mix(table.concat(c, ",", 5, 9))
  ok, e = pcall(table.concat, {1, {}, 3}); err(e)
  local grid = {}
  for i = 1, 20 do grid[i] = {} for j = 1, 20 do grid[i][j] = i * j end end
  local g = 0
  for i = 1, 20 do for j = 1, 20 do g = g + grid[j][i] * (i - j) end end
  mix(g)
  mix(select("#", table.unpack(t, 1, 10)))
  mix(next({})); mix(rawlen(t)); mix(rawequal(t, t)); mix(rawget(m, 1))
  local w = setmetatable({}, { __mode = "k" })
  return s, #t, g
end
run("s$id", s$id, $N)
]], { id = id, N = N(100, 1500), M = rnd(1, 9), K = rnd(2, 30), INS = pick({ "i", "i * 0.5", "'s'" }), P = rnd(3, 97), Q = rnd(11, 1009) })
end

-- Strings
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local parts = {}
  for i = 1, n do
    local s = tostring(i * $M) .. ":" .. string.format("%5.2f|%d|%x|%q", i / 7, i, i, "a\n" .. i)
    parts[#parts + 1] = s:sub(2, -2):upper()
    if i % 50 == 0 then
      mix(s:find(":", 1, true)); mix(s:match("(%d+)")); mix(#s:rep(3, "-"))
      mix(s:gsub("%d", "x")); mix(s:byte(1, 3)); mix(string.char(65 + i % 26))
    end
  end
  mix(#table.concat(parts))
  mix(tostring(1e15)); mix(tostring(1e100)); mix(tostring(-0.0)); mix(tostring(2^63)); mix(tostring(math.mininteger))
  mix(tostring(0.1)); mix(tostring(100 // 1e0)); mix(3 .. ""); mix(3.0 .. ""); mix(-0.0 .. "")
  mix(tonumber("0x10")); mix(tonumber("  12  ")); mix(tonumber("1e2")); mix(tonumber("abc")); mix(tonumber("10", 2))
  mix(tonumber("9223372036854775808")); mix(tonumber("0x7fffffffffffffff")); mix(tonumber("0xffffffffffffffff"))
  mix("10" + 1); mix("0x10" * 2); mix("3" // 2); mix("3.5" % 2); mix(10 .. 20)
  mix("a" < "b"); mix("a" < "B"); mix("" < "a"); mix("abc" == "abc")
  local ok, e = pcall(function() return "a" + 1 end); err(e)
  ok, e = pcall(function() return {} .. "x" end); err(e)
  ok, e = pcall(function() return "a" < 1 end); err(e)
  ok, e = pcall(function() return #5 end); err(e)
  ok, e = pcall(string.rep); err(e)
  ok, e = pcall(string.format, "%d", 1.5); err(e)
  ok, e = pcall(string.format, "%d", "x"); err(e)
  mix(string.format("%d", 3.0)); mix(string.format("%s %s", 1, 1.0)); mix(string.format("%g", 1e20))
  mix(string.format("%10.3s|", "abcdef")); mix(string.format("%-5d|", 42)); mix(string.format("%+d", 5))
  mix(("x"):rep(0)); mix(("abc"):sub(-100, 100)); mix(("abc"):sub(3, 2)); mix(#("abc"):reverse())
  for w in string.gmatch("one two  three", "%a+") do mix(w) end
  mix(("hello world"):gsub("o", { o = "0" })); mix(("hello"):gsub("l", function(c) return c:upper() end))
  mix(string.len("\0\0")); mix(("a,b,,c"):find(",,"))
  return #parts
end
run("s$id", s$id, $N)
]], { id = id, N = N(100, 1500), M = rnd(1, 13) })
end

-- Closures, upvalues, varargs, multiple returns
T[#T + 1] = function(id)
  return tpl([[
local function mk$id(step)
  local c = 0
  local function inc() c = c + step; return c end
  local function get() return c end
  return inc, get
end
local function va$id(...)
  local a, b = ...
  local t = {...}
  local p = table.pack(...)
  return select("#", ...), a, b, #t, p.n, (select(-1, ...))
end
local function mr$id(n) return n, n + 1, n + 2 end
local function s$id(n)
  local inc, get = mk$id($STEP)
  local fs = {}
  for i = 1, n do
    inc()
    if i % 20 == 0 then fs[#fs + 1] = function() return i + get() end end
  end
  local s = 0
  for _, f in ipairs(fs) do s = s + f() end
  mix(s); mix(get())
  for i = 1, n // 10 do
    mix(va$id(i, nil, i + 1)); mix(va$id(i)); mix(va$id(mr$id(i)))
    local x, y, z, w = mr$id(i)
    mix(x); mix(y); mix(z); mix(w)
    local t = { mr$id(i), mr$id(i) }
    mix(#t)
    local u = { (mr$id(i)) }
    mix(#u)
  end
  local ok, e = pcall(va$id); err(e)
  ok, e = pcall(select, 0); err(e)
  mix(select(2, "a", "b", "c")); mix(select("#")); mix(select(-2, 1, 2, 3))
  local function tail(k, acc) if k == 0 then return acc end return tail(k - 1, acc + k) end
  mix(tail(n * 3, 0))
  local sh = 0
  local function a1() sh = sh + 1; return sh end
  local function a2() sh = sh * 2; return sh end
  for i = 1, n do a1(); if i % 10 == 0 then a2() end; if sh > 1e9 then sh = 0 end end
  mix(sh)
  return s
end
run("s$id", s$id, $N)
]], { id = id, STEP = rnd(1, 5), N = N(100, 2000) })
end

-- Recursion (bounded depth) incl. stack overflow caught
T[#T + 1] = function(id)
  local so = chance(30) and not hookmode  -- overflow depth is implementation defined; slow under hooks
  return tpl([[
local function fib$id(n) if n < 2 then return n end return fib$id(n - 1) + fib$id(n - 2) end
local function ack$id(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack$id(m - 1, 1) end
  return ack$id(m - 1, ack$id(m, n - 1))
end
local function deep$id(n) if n == 0 then return 0 end return 1 + deep$id(n - 1) end
local function mut$id(n) if n == 0 then return "done" end return (mutx_$id or function(k) return mut$id(k) end)(n - 1) end
local function s$id(n)
  local s = 0
  for i = 1, $R do s = s + fib$id(n % 14) end
  mix(s); mix(ack$id(2, n % 6)); mix(deep$id($D))
  local ok, e = pcall(deep$id, $DD); mix(ok); if not ok then err(e) end
  mix(mut$id(100))
  $SO
  return s
end
run("s$id", s$id, $N)
]], { id = id, N = rnd(5, 60), R = N(20, 200), D = rnd(10, 190), DD = hookmode and 3000 or 150000, SO = so and
    "local function inf(n) return 1 + inf(n + 1) end local ok2, e2 = pcall(inf, 1); mix(ok2); err(e2)" or "" })
end

-- pcall / error with non-string values, levels, xpcall
T[#T + 1] = function(id)
  return tpl([[
local function thrower$id(k)
  if k % 7 == 0 then error({code = k})
  elseif k % 7 == 1 then error(k)
  elseif k % 7 == 2 then error(nil)
  elseif k % 7 == 3 then error("msg" .. k, 0)
  elseif k % 7 == 4 then error("lvl2", 2)
  elseif k % 7 == 5 then error(setmetatable({}, {__tostring = function() return "custom" end}))
  elseif k % 7 == 6 then local x = nil; return x.field
  end
end
local function wrap$id(k) thrower$id(k) end
local function s$id(n)
  local cnt = 0
  for i = 1, n do
    local ok, e = pcall(wrap$id, i)
    if not ok then
      cnt = cnt + 1
      if type(e) == "table" then mix(e.code) elseif i % 7 == 5 then mix(tostring(e)) else err(e) end
    end
  end
  mix(cnt)
  local ok, e = xpcall(wrap$id, function(m) return "H:" .. tostring(type(m) == "string" and m or type(m)) end, 6)
  err(e)
  ok, e = xpcall(wrap$id, debug.traceback, 6)
  err(e)
  ok, e = xpcall(wrap$id, function(m) error("in handler") end, 6)
  mix(ok); mix(type(e))
  ok, e = pcall(error); mix(ok); mix(e)
  ok, e = pcall(pcall); err(e)
  ok, e = pcall(setmetatable, 1, 2); err(e)
  ok, e = pcall(function() return 1 < {} end); err(e)
  ok, e = pcall(function() return {} < {} end); err(e)
  ok, e = pcall(function() return nil .. "x" end); err(e)
  ok, e = pcall(function() local a = "x"; return a.b.c end); err(e)
  ok, e = pcall(function() undefinedglobal.x = 1 end); err(e)
  ok, e = pcall(function() undefinedfn() end); err(e)
  ok, e = pcall(function() local t = {}; t.a.b() end); err(e)
  ok, e = pcall(function() local t = {}; t:nomethod() end); err(e)
  ok, e = pcall(function() return 1 // 0 end); err(e)
  ok, e = pcall(function() return 1 % 0 end); err(e)
  ok, e = pcall(function() return 1.5 // 0, 1 % 0.0 end); err(e)
  ok, e = pcall(function() return 2^53 | 0, 1.5 | 0 end); err(e)
  ok, e = pcall(function() return "a" | 0 end); err(e)
  ok, e = pcall(function() return math.maxinteger + 1.0 | 0 end); err(e)
  return cnt
end
run("s$id", s$id, $N)
]], { id = id, N = N(100, 1500) })
end

-- Coroutines with yield inside loops
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local co = coroutine.create(function(a, b)
    local acc = a
    for i = 1, n do
      local x = coroutine.yield(acc, i)
      acc = acc + (x or 0) + b
      if i % $E == 0 then
        local ok, e = pcall(function() coroutine.yield("in pcall", i); error("boom" .. i) end)
        mix(ok); err(e)
      end
    end
    return "fin", acc
  end)
  local v1, v2 = 0, 0
  local out = { coroutine.resume(co, 1, 2) }
  local steps = 0
  while coroutine.status(co) == "suspended" do
    steps = steps + 1
    mix(out[1]); mix(out[2]); mix(out[3])
    out = { coroutine.resume(co, steps % 5) }
  end
  mix(out[1]); mix(out[2]); mix(out[3]); mix(coroutine.status(co))
  local ok, e = coroutine.resume(co); mix(ok); err(e)
  local gen = coroutine.wrap(function() for i = 1, n do coroutine.yield(i * i) end end)
  local s = 0
  for i = 1, n do s = s + gen() end
  mix(s)
  local w = coroutine.wrap(function() error({}) end)
  ok, e = pcall(w); mix(ok); mix(type(e))
  w = coroutine.wrap(function() local x = nil; return x + 1 end)
  ok, e = pcall(w); err(e)
  mix(coroutine.isyieldable()); mix(select(2, coroutine.running()))
  local c2 = coroutine.create(function() coroutine.yield(1) end)
  coroutine.resume(c2)
  mix(coroutine.close(c2)); mix(coroutine.status(c2))
  ok, e = pcall(coroutine.yield, 1); err(e)
  local c3 = coroutine.wrap(function(...) local a = {...}; while true do a = {coroutine.yield(#a)} end end)
  for i = 1, n // 4 do mix(c3(table.unpack({1, 2, 3}, 1, i % 4))) end
  local c4 = coroutine.create(function() local t <close> = setmetatable({}, {__close = function() mix("closed") end}); coroutine.yield(1) end)
  coroutine.resume(c4); mix(coroutine.close(c4))
  local c5 = coroutine.create(function() return coroutine.resume(coroutine.running()) end)
  mix(coroutine.resume(c5))
  return steps
end
run("s$id", s$id, $N)
]], { id = id, N = N(50, 600), E = rnd(3, 20) })
end

-- Metamethods
T[#T + 1] = function(id)
  return tpl([[
local V$id = {}
V$id.__index = V$id
local function new$id(x) return setmetatable({x = x}, V$id) end
V$id.__add = function(a, b)
  local ax = type(a) == "table" and a.x or a
  local bx = type(b) == "table" and b.x or b
  return new$id(ax + bx)
end
V$id.__sub = function(a, b) return new$id(a.x - (type(b) == "table" and b.x or b)) end
V$id.__mul = function(a, b) return a.x * (type(b) == "table" and b.x or b) end
V$id.__unm = function(a) return new$id(-a.x) end
V$id.__eq = function(a, b) return a.x == b.x end
V$id.__lt = function(a, b) return a.x < b.x end
V$id.__le = function(a, b) return a.x <= b.x end
V$id.__concat = function(a, b) return (type(a) == "table" and a.x or a) .. "|" .. (type(b) == "table" and b.x or b) end
V$id.__call = function(self, k, ...) return self.x + k, select("#", ...) end
V$id.__len = function(a) return a.x end
V$id.__idiv = function(a, b) return "idiv" end
V$id.__band = function(a, b) return "band" end
V$id.__shl = function(a, b) return "shl" end
V$id.__bnot = function(a) return "bnot" end
V$id.__close = function() mix("close") end
V$id.__tostring = function(a) return "V(" .. a.x .. ")" end
local function s$id(n)
  local acc = new$id(0)
  local cnt = 0
  for i = 1, n do
    acc = acc + i
    acc = acc - 1
    if acc == new$id(acc.x) then cnt = cnt + 1 end
    if acc < new$id(i * 2) then cnt = cnt + 2 end
    if acc <= new$id(i) then cnt = cnt + 3 end
    if i % 40 == 0 then
      mix(acc .. "s"); mix("s" .. acc); mix(1 .. acc); mix(acc(i, 1, 2)); mix(#acc); mix(acc * 2)
      mix(-acc == new$id(-acc.x)); mix(acc // 1); mix(acc & 1); mix(1 << acc); mix(~acc)
      mix(tostring(acc)); mix(acc ~= new$id(acc.x + 1)); mix(acc > new$id(3)); mix(acc >= new$id(3))
    end
  end
  mix(acc.x); mix(cnt)
  local prox = setmetatable({}, {
    __index = function(t, k) return k end,
    __newindex = function(t, k, v) rawset(t, k, v * 2) end })
  for i = 1, n // 5 do
    prox[i] = i; mix(prox[i]); mix(prox[i + 0.5]); mix(prox.name)
  end
  local ro = setmetatable({}, {__index = {a = 1, b = setmetatable({}, {__index = {c = 3}})}, __newindex = function() error("readonly") end})
  mix(ro.a); mix(ro.b.c)
  local ok, e = pcall(function() ro.z = 1 end); err(e)
  ok, e = pcall(function() return acc + {} end); err(e)
  ok, e = pcall(function() return new$id(1) < 1 end); err(e)
  ok, e = pcall(function() return {} .. {} end); err(e)
  ok, e = pcall(function() local t = setmetatable({}, {__index = function(t, k) error("idx " .. k) end}); return t.foo end); err(e)
  local lk = setmetatable({}, {__metatable = "locked"})
  mix(getmetatable(lk)); ok, e = pcall(setmetatable, lk, {}); err(e)
  do local c <close> = new$id(5) end
  ok, e = pcall(function() local c <close> = 42 end); err(e)
  local ec = setmetatable({}, {__eq = function() return true end})
  mix(ec == setmetatable({}, getmetatable(ec))); mix(ec == 1); mix(ec ~= {})
  return cnt
end
run("s$id", s$id, $N)
]], { id = id, N = N(100, 1500) })
end

-- Bitwise and shifts
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local a, b = $A, $B
  for i = 1, n do
    a = (a ~ (a << 13)) & 0xffffffffffff
    a = a ~ (a >> 7)
    b = (b * 6364136223846793005 + 1442695040888963407)
    local sh = (i % 130) - 33
    local x = b >> sh
    local y = b << sh
    a = a | (x & 0xff) ~ (y >> 56)
    if i % 100 == 0 then mix(a); mix(x); mix(y); mix(~b); mix(b // 3); mix(b % 1000); mix(b % -7); mix(b // -7) end
  end
  mix(1 << 63); mix(1 << 64); mix(-1 >> 1); mix(-1 >> 63); mix(1 << -1); mix(2 >> -1)
  mix(3.0 | 0); mix("3" | 0); mix(2^31 | 0); mix(~0); mix(math.mininteger // -1); mix(math.mininteger % -1)
  mix(math.maxinteger * 2); mix(math.mininteger - 1); mix(-math.mininteger); mix(math.abs(math.mininteger))
  mix(7 // 2); mix(-7 // 2); mix(7 // -2); mix(-7 % 3); mix(7 % -3); mix(7.5 // 2); mix(-7.5 % 2); mix(5 % math.huge); mix(-5 % math.huge)
  mix(5.0 // 0.0); mix(-5.0 // 0.0); mix(0.0 / 0.0 ~= 0.0 / 0.0); mix(3 / 2); mix(4 / 2); mix(2^2); mix(2^0.5)
  local ok, e = pcall(function() return 1.5 & 1 end); err(e)
  ok, e = pcall(function() return 2^64 | 0 end); err(e)
  ok, e = pcall(function() return {} & 1 end); err(e)
  ok, e = pcall(function() return "1.5" & 1 end); err(e)
  ok, e = pcall(function() return nil | 1 end); err(e)
  return a, b
end
run("s$id", s$id, $N)
]], { id = id, N = N(200, 3000), A = pick({ "1", "12345", "0x1234567" }), B = pick({ "1", "99", "-5" }) })
end

-- Comparison matrix and arithmetic matrix over special values
local VALS = { "0", "1", "-1", "2", "3", "math.maxinteger", "math.mininteger", "0.0", "(-0.0)", "0.5", "-1.5",
  "2^53", "2^63", "(-2^63)", "math.huge", "(-math.huge)", "(0/0)", "'10'", "'x'", "nil", "true", "1e308", "3.0",
  "(2^53 + 1)", "9007199254740993", "9223372036854775807.0" }
T[#T + 1] = function(id)
  local ops = { "+", "-", "*", "/", "//", "%", "^", "&", "|", "~", "<<", ">>", "..", "==", "<", "<=", "~=", ">", ">=", "and", "or" }
  local vals = {}
  for i = 1, 11 do vals[#vals + 1] = pick(VALS) end
  local fs = {}
  for _, op in ipairs(ops) do
    if chance(55) then
      fs[#fs + 1] = tpl("  { function(a, b) return a $op b end, '$op' },", { op = op })
    end
  end
  fs[#fs + 1] = "  { function(a, b) return -a end, 'neg' },"
  fs[#fs + 1] = "  { function(a, b) return ~a end, 'bnot' },"
  fs[#fs + 1] = "  { function(a, b) return a // b end, 'idiv' },"
  fs[#fs + 1] = "  { function(a, b) return math.fmod(a, b) end, 'fmod' },"
  fs[#fs + 1] = "  { function(a, b) return math.max(a, b), math.min(a, b) end, 'maxmin' },"
  fs[#fs + 1] = "  { function(a, b) return math.tointeger(a), math.type(b), math.ult(a, b) end, 'tointeger' },"
  fs[#fs + 1] = "  { function(a, b) return math.floor(a), math.ceil(b) end, 'floor' },"
  return tpl([[
local F$id = {
$FS
}
local V$id = { $VALS }
local function s$id(reps)
  for r = 1, reps do
    for fi = 1, #F$id do
      local f = F$id[fi][1]
      for i = 1, #V$id + 1 do
        for j = 1, #V$id + 1 do
          local ok, a, b, c = pcall(f, V$id[i], V$id[j])
          if ok then mix(a); mix(b); mix(c) else err(a) end
        end
      end
    end
  end
end
run("s$id", s$id, $R)
]], { id = id, FS = table.concat(fs, "\n"), VALS = table.concat(vals, ", "), R = rnd(3, 12) // SCALE + 1 })
end

-- Integer/float for loop accumulations with mixed types
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local si, sf = 0, 0.0
  for i = 1, n do
    si = si + i * i
    sf = sf + i / 3
    if i % 3 == 0 then si = si - sf // 1 end
    if si > 1e15 then si = si // 1000 end
  end
  mix(si); mix(sf)
  local x = 1.0
  for i = 1, n do x = x * 1.01; if x > 1e6 then x = x / 1e6 end end
  mix(x)
  local mn, mx = math.huge, -math.huge
  for i = 1, n do local v = (i * 7919) % 1013; mn = math.min(mn, v); mx = math.max(mx, v) end
  mix(mn); mix(mx)
  local s = 0
  for i = n, 1, -1 do s = s + (i & 3) end
  mix(s)
  for x = 0.0, 1.0, 1 / $D do s = s + x end
  mix(s)
  for i = math.maxinteger - $K, math.maxinteger do s = s ~ i end
  mix(s)
  return si
end
run("s$id", s$id, $N)
]], { id = id, N = N(300, 4000), D = rnd(2, 40), K = rnd(0, 30) })
end

-- to-be-closed and const, shadowing, integer keys loops, ipairs/pairs/next
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local K <const> = $K
  local total = 0
  for i = 1, n do
    local x <close> = setmetatable({}, {__close = function() total = total + 1 end})
    local y = K + i
    do local y = y * 2; total = total + y end
    if i % 13 == 0 then
      local t = {10, 20, 30, nil, 50}
      for k, v in ipairs(t) do total = total + k * v end
      local c = 0
      for k, v in pairs({1, 2, 3, a = 4}) do c = c + v end
      total = total + c
      local k, v = next({}, nil); mix(k)
    end
  end
  mix(total)
  local t = {}
  for i = 1, 10 do t[i] = i end
  for i = 1, 10 do t[i] = nil end
  mix(next(t)); mix(#t)
  for k, v in pairs(setmetatable({}, {__pairs = function(t) return function(_, k) if not k then return 1, 2 end end, t, nil end})) do mix(k); mix(v) end
  local ok, e = pcall(function() for k in pairs(nil) do end end); err(e)
  ok, e = pcall(function() for k in 5 do end end); err(e)
  ok, e = pcall(next, {}, "nokey"); err(e)
  ok, e = pcall(ipairs); err(e)
  return total
end
run("s$id", s$id, $N)
]], { id = id, K = rnd(1, 99), N = N(100, 2000) })
end

-- Integer keyed tables, float keys normalisation in loops
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local t = {}
  for i = 1, n do
    t[i + 0.0] = i
    t[-i] = i
    t[i * 2^40] = i
  end
  local s = 0
  for i = 1, n do s = s + t[i] + t[-i] + (t[i * 2^40] or 0) + (t[i + 0.5] or 0) end
  mix(s); mix(#t)
  local h = {}
  for i = 1, n do h[i % $K] = (h[i % $K] or 0) + 1; h[(i % $K) + 0.5] = i end
  local ks = {}
  for k in pairs(h) do ks[#ks + 1] = k end
  table.sort(ks)
  for i = 1, #ks, 3 do local kk = ks[i]; mix(kk); mix(h[kk]) end
  local r = {}
  for i = n, 1, -1 do r[i] = i end
  mix(#r)
  local u = {n = 0}
  for i = 1, n do u.n = u.n + 1; u["f" .. (i % 5)] = i end
  mix(u.n); mix(u.f0); mix(u.f4)
  local big = {}
  big[math.maxinteger] = 1; big[math.mininteger] = 2; big[2^63] = 3; big[-2^63] = 4
  mix(big[math.maxinteger]); mix(big[math.mininteger]); mix(big[2^63]); mix(big[-2^63])
  return s
end
run("s$id", s$id, $N)
]], { id = id, K = rnd(3, 40), N = N(100, 2000) })
end


------------------------------------------------------------------ T12: mid-run state changes
-- (all of these keep their own counters and fold them into the checksum; hooks are removed with rehook())

-- a mask/count combination for a mid-run hook
local function hookspec()
  return pick({ { '""', rnd(1, 40) }, { '""', rnd(1, 3) }, { '"l"', 0 }, { '"l"', rnd(1, 9) }, { '"c"', 0 }, { '"cr"', 0 }, { '"r"', 0 } })
end

-- hook installed in the middle of a hot loop (from the loop itself), removed after a few iterations; run twice so the
-- second run enters compiled code with the hook installed mid-run again
T[#T + 1] = function(id)
  local h = hookspec()
  local a = rnd(5, 60)
  return tpl([[
local function s$id(n)
  local acc, cnt = 0, 0
  for i = 1, n do
    acc = acc + i * 3 - (acc >> 4)
    if i == $A then debug.sethook(function(ev, l) cnt = cnt + 1 end, $MASK, $CNT) end
    acc = acc ~ (i & 7)
    if i == $B then rehook() end
  end
  mix(cnt)
  return acc
end
run("s$id", s$id, $N)
run("s$id", s$id, $M)
run("s$id", s$id, $N)
]], { id = id, A = a, B = a + rnd(1, 80), MASK = h[1], CNT = h[2], N = N(150, 1500), M = N(150, 700) })
end

-- hook installed and removed from callees
T[#T + 1] = function(id)
  local h = hookspec()
  local a = rnd(5, 60)
  return tpl([[
local hk$id = 0
local function inst$id(m, c) debug.sethook(function() hk$id = hk$id + 1 end, m, c) end
local function s$id(n)
  local a, b = 1, 0
  for i = 1, n do
    a = (a * 31 + i) % 1000003
    if i == $A then inst$id($MASK, $CNT) end
    b = b + a % 5
    if i == $B then rehook() end
  end
  mix(hk$id)
  return a, b
end
run("s$id", s$id, $N)
run("s$id", s$id, $N)
]], { id = id, A = a, B = a + rnd(1, 80), MASK = h[1], CNT = h[2], N = N(150, 1500) })
end

-- hook installed from a finalizer that runs during collectgarbage() inside the loop
T[#T + 1] = function(id)
  local a = rnd(5, 50)
  return tpl([[
local function s$id(n)
  local acc, hk = 0, 0
  for i = 1, n do
    acc = acc + i
    if i == $A then
      setmetatable({}, { __gc = function() debug.sethook(function() hk = hk + 1 end, "", $C) end })
    end
    if i % $G == 0 then collectgarbage() end
    acc = acc ~ (i << 1)
    if i == $B then rehook() end
  end
  rehook()
  mix(hk)
  return acc
end
collectgarbage("stop")
run("s$id", s$id, $N)
collectgarbage("restart")
]], { id = id, A = a, B = a + rnd(30, 100), C = rnd(1, 20), G = rnd(3, 11), N = N(120, 600) })
end

-- debug.setlocal from a callee / from a hook function in the middle of the loop: the local changes type
-- (int -> float -> string -> nil ...) and the loop must go on with the new value
T[#T + 1] = function(id)
  local vals = { "1", "2.5", "'7'", "nil", "true", "1 << 40", "-0.0", "'x'", "math.maxinteger", "{}" }
  local seq = {}
  for k = 1, rnd(3, 6) do seq[#seq + 1] = pick(vals) end
  local via = pick({ "callee", "hook", "callee" })
  local guard = chance(60)
  return tpl([[
local seq$id = { $SEQ }
local nseq$id = $NSEQ
local function poke$id(k) setl(2, "acc", seq$id[k % nseq$id + 1]) end
local function s$id(n)
  local acc, t = 0, 0
  for i = 1, n do
    $POKE
    $STEP
    t = t + 1
  end
  rehook()
  mix(t)
  return acc
end
run("s$id", s$id, $N)
run("s$id", s$id, $N)
]], {
    id = id, SEQ = table.concat(seq, ", "), NSEQ = #seq,
    N = N(100, 1000),
    POKE = via == "callee" and ("if i % " .. rnd(3, 40) .. " == 0 then poke" .. id .. "(i) end")
        or ("if i == " .. rnd(3, 40) .. " then local c = 0; debug.sethook(function() c = c + 1; poke" .. id ..
            "(c) end, '', " .. rnd(1, 30) .. ") end if i == " .. rnd(41, 90) .. " then rehook() end"),
    STEP = guard and "acc = (tonumber(acc) or 0) + 1" or "acc = acc + 1",
  })
end

-- debug.upvaluejoin / debug.setupvalue aliasing a captured local while its frame is running
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local a, b = 0, 0
  local function ha() a = a + $K end
  local function gb() return b end
  local function sb(v) b = v end
  for i = 1, n do
    a = a + 1; b = b + 2
    if i % $P == 0 then ha() end
    if i == $A then debug.upvaluejoin(ha, 1, gb, 1) end
    if i == $B then debug.setupvalue(sb, 1, $V) end
    if i == $C then b = b + 0.5 end
    if i == $D then debug.upvaluejoin(ha, 1, sb, 1) end
  end
  mix(a); mix(b)
  return a, b
end
run("s$id", s$id, $N)
run("s$id", s$id, $N)
]], { id = id, K = rnd(1, 100), P = rnd(2, 9), A = rnd(5, 60), B = rnd(5, 90), C = rnd(5, 90), D = rnd(40, 120),
    V = pick({ "0.5", "7", "'3'", "nil", "-1" }), N = N(150, 900) })
end

-- coroutine.wrap / yield inside hot loops with a type change in the middle
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, K)
  local co = coroutine.create(function()
    local acc, f = 0, 0.5
    for i = 1, n do
      acc = acc + i; f = f * 1.0001 + acc * 1e-9
      if i % K == 0 then coroutine.yield(acc) end
      if i == n // 2 then acc = acc + 0.25 end
    end
    return acc, f
  end)
  local tot = 0
  for _ = 1, n + 2 do
    local ok, v, w = coroutine.resume(co)
    if not ok then err(v); break end
    if type(v) == "number" then tot = tot + (v // 1) end
    if w then mix(w) end
    if coroutine.status(co) == "dead" then break end
  end
  return tot
end
run("s$id", s$id, $N, 1)
run("s$id", s$id, $N, 7)
run("s$id", s$id, $N, 1000)
]], { id = id, N = N(150, 1500) })
end

-- type-unstable accumulators: the type changes every K iterations, K in 1, 7, 1000
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, K)
  local acc = 0
  for i = 1, n do
    local phase = (i // K) % 4
    if phase == 0 then acc = acc + 1
    elseif phase == 1 then acc = acc * 1.5
    elseif phase == 2 then acc = tostring(acc // 1) .. ""
    else acc = nil; acc = i end
    acc = acc + 1
  end
  return acc
end
run("s$id", s$id, $N, 1)
run("s$id", s$id, $N, 7)
run("s$id", s$id, $N, 1000)
run("s$id", s$id, $N, 1)
]], { id = id, N = N(300, 3000) })
end

-- bounds and step of numeric loops from variables of changing type; integer overflow loops
T[#T + 1] = function(id)
  return tpl([[
local function s$id(a, b, c)
  local s = 0
  for i = a, b, c do s = s + i; if s > 1e6 or s < -1e6 then break end end
  return s
end
local function o$id(n)
  local x, y, z = math.maxinteger - $K, 1, math.mininteger + $K
  for i = 1, n do
    x = x + 3; z = z - 3; y = y * 3 + i
    if x < 0 then x = x & 0xff end
  end
  local s = 0
  for i = math.maxinteger - 40, math.maxinteger, $S do s = s ~ i end
  for i = math.mininteger + 40, math.mininteger, -$S do s = s ~ i end
  return x, y, z, s
end
local cases$id = { { 1, 50, 1 }, { 1.0, 50, 1 }, { 1, 50.5, 1 }, { 1, 50, 2.0 }, { 50, 1, -1 }, { "1", 10, 1 },
  { 1, "10", 1 }, { 1, 10, 0 }, { math.maxinteger - 5, math.maxinteger, 3 }, { math.mininteger, math.mininteger + 3, 1 },
  { 1, 1 / 0, 1 }, { 3, 0 / 0, 1 }, { -0.0, 3, 1 } }
local function r$id()
  for rep = 1, $N do
    for _, c in ipairs(cases$id) do
      local ok, v = pcall(s$id, c[1], c[2], c[3])
      if ok then mix(v) else err(v) end
    end
  end
end
run("r$id", r$id)
run("o$id", o$id, $M)
run("o$id", o$id, $M)
]], { id = id, K = rnd(0, 30), S = rnd(1, 9), N = N(20, 200), M = N(40, 400) })
end

------------------------------------------------------------------ T4b: floats in XMM registers
-- NaN / inf / -0.0 / 2^53 through float arithmetic, int-float conversion, compares with every immediate form
T[#T + 1] = function(id)
  return tpl([[
local function s$id(a, b)
  local nan, inf = 0.0 / 0.0, math.huge
  local x, y, z, w = a, b, -0.0, 1.5
  local c = 0
  for i = 1, $N do
    x = x * $KA + y / ($KB + i)
    y = y - w + z
    z = -z * $KC
    if x ~= x then x = $E0; c = c + 1 end
    if y < x then w = w + 0.25 elseif y == x then w = w - x else w = -w end
    if x >= inf or x <= -inf then x = 1.0; c = c + 2 end
    if z == 0 then c = c + 4 end
    if w ~= w then w = 0.5 end
    if x > $IM then c = c + 8 elseif x <= $IM then c = c + 16 end
    if y == 2 then c = c + 32 end
    z = z + i * 0.125 - (i // 3)
  end
  return x, y, z, w, c
end
local cs$id = { { 0.0, -0.0 }, { 1.5, 2 }, { math.huge, -math.huge }, { 0.0 / 0.0, 1 }, { 2^53, 1 }, { -2^53, 0.1 },
  { 3, 4 }, { math.maxinteger, math.mininteger }, { -0.0, 0.0 }, { 1e308, 1e308 } }
local function r$id()
  for rep = 1, $R do
    for _, c in ipairs(cs$id) do
      local ok, a, b, c2, d, e = pcall(s$id, c[1], c[2])
      if ok then mix(a); mix(b); mix(c2); mix(d); mix(e) else err(a) end
    end
  end
end
run("r$id", r$id)
]], { id = id, N = N(20, 120), R = N(20, 120), KA = pick({ "1.5", "0.5", "-1.25", "3", "1e10", "1e-10", "-1" }),
    KB = pick({ "1", "0.5", "-3", "2" }), KC = pick({ "-1", "1.0001", "0.5", "-0.0" }),
    E0 = pick({ "0.0", "1.0", "-0.0", "1e300" }), IM = pick({ "0", "3", "-5", "100", "-128" }) })
end

-- numeric `for` loops with float init / limit / step (accumulating step), NaN and infinite bounds, 2^53, overflow
T[#T + 1] = function(id)
  return tpl([[
local function g$id(a, b, c, lim)
  local s, k = 0.0, 0
  for x = a, b, c do s = s + x; k = k + 1; if k >= lim then break end end
  local t = 0
  for x = a, b, c do t = t + 1; if t >= lim then break end end
  return s, k, t
end
local cases$id = { { 0, 1, 0.1 }, { 1, 0, -0.1 }, { 0.5, 10, 1.5 }, { 10, 0.5, -0.75 }, { 1, 3.5, 1 }, { 1.0, 3, 1 },
  { 1, 3, 1.0 }, { 0, 1 / 0, 1 }, { 0, 1e300, 1e299 }, { -0.0, 2, 1 }, { 0, -0.0, 1 }, { 1, 0 / 0, 1 }, { 0 / 0, 1, 1 },
  { 1, 2, 0 / 0 }, { 2^53 - 2, 2^53 + 2, 1 }, { 2^53, 2^53 + 4, 1 }, { math.maxinteger - 3, math.maxinteger, 1 },
  { math.maxinteger - 3.0, math.maxinteger + 1.0, 1 }, { math.mininteger, math.mininteger + 3, 1 }, { 3, 1, 1 },
  { 1, 3, -1 }, { -3, 3, 2 }, { 3, -3, -2.5 }, { 1, 10, 3 }, { 1.5, 1.5, 1 }, { 1, 5, 1e-300 }, { 1, 2, 0.0 },
  { 1, "2", 0.5 }, { 1, 2.5, "1" }, { 0.1, $L, 0.1 }, { $L, 0, -0.5 } }
local function r$id()
  for rep = 1, $R do
    for _, c in ipairs(cases$id) do
      local ok, v, k, t = pcall(g$id, c[1], c[2], c[3], $LIM)
      if ok then mix(v); mix(k); mix(t) else err(v) end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(10, 60), L = pick({ "2", "3.3", "7", "1.05" }), LIM = rnd(5, 40) })
end

-- int <-> float conversions near 2^53 and the integer limits inside hot loops (mixed compares fall back)
T[#T + 1] = function(id)
  return tpl([[
local function s$id(base, n)
  local s, t, u = 0.0, 0, 0
  for i = 1, n do
    local v = (base + i) + 0.0
    s = s + v / 3 - (base - i) * 0.5
    if v == base + i then t = t + 1 end
    if v < base + i then u = u + 1 end
    if v > base then t = t + 2 end
    s = s - (2^53 + i)
  end
  return s, t, u
end
local bs$id = { 0, 9007199254740988, math.maxinteger - 200, math.mininteger, -9007199254740988, 2^53, 1e15, 0.5 }
local function r$id()
  for rep = 1, $R do
    for _, b in ipairs(bs$id) do
      local ok, a, c, d = pcall(s$id, b, $N)
      if ok then mix(a); mix(c); mix(d) else err(a) end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(5, 50), N = rnd(20, 300) })
end

-- a float loop whose operand turns into a string / table / nil / integer at iteration K (coercion falls back)
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, k, repl)
  local x, y = 1.5, 0.0
  for i = 1, n do
    if i == k then x = repl end
    y = y + x * 2 - 0.25
    x = x + 0.5
    if i == k + 1 then x = 2.5 end
  end
  return y, x
end
local rs$id = { "10", "0x10", " 7 ", "1e2", 3, -0.0, "abc", {}, false }
local function r$id()
  for rep = 1, $R do
    for _, r in ipairs(rs$id) do
      for _, k in ipairs{ 1, 2, 3, $K } do
        local ok, a, b = pcall(s$id, $N, k, r)
        if ok then mix(a); mix(b) else err(a) end
      end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(5, 40), N = rnd(10, 150), K = rnd(4, 100) })
end

------------------------------------------------------------------ T5: inline table accesses in regions
-- array part: int / float elements, holes, growth, out-of-range / zero / negative / huge / float / NaN keys
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, off, step)
  local t, u = {}, {}
  for i = 1, $SZ do t[i] = i; u[i] = i * 0.5 end
  local acc, c = 0.0, 0
  local k = off
  for i = 1, n do
    local v = t[k]
    if v then acc = acc + v; c = c + 1 else acc = acc - 1 end
    local w = u[k]
    if w then acc = acc + w * 0.5 end
    if v then t[k] = v + 1 end
    if i % $HOLE == 0 then t[k] = nil end
    k = k + step
    if k > $SZ + 3 then k = off end
  end
  return acc, c, #t, t[1], u[2]
end
local offs$id = { 1, 0, -1, -5, 3, $SZ, $SZ + 1, 1 << 40, math.maxinteger, math.mininteger, 2.0, 2.5, 0/0 }
local function r$id()
  for rep = 1, $R do
    for _, o in ipairs(offs$id) do
      for _, st in ipairs{ 1, 2, 0.5, 1.0 } do
        local ok, a, b, c, d, e = pcall(s$id, $N, o, st)
        if ok then mix(a); mix(b); mix(c); mix(d); mix(e) else err(a) end
      end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(2, 12), N = rnd(15, 120), SZ = rnd(4, 40), HOLE = rnd(2, 9) })
end

-- hash part: constant fields of every size (collision chains), absent fields, insertion / removal / rehash mid-loop
T[#T + 1] = function(id)
  local nf = rnd(1, 30)
  local decl = {}
  for i = 1, nf do decl[#decl + 1] = (i % 3 == 0) and ("f" .. i .. " = " .. i .. ".5") or ("f" .. i .. " = " .. i) end
  local fa, fb, fc = "f" .. rnd(1, nf), "f" .. rnd(1, nf), "f" .. rnd(1, nf)
  return tpl([[
local function s$id(n, at)
  local t = { $DECL }
  local acc = 0.0
  for i = 1, n do
    acc = acc + t.$FA + t.$FB * 0.5
    t.$FC = t.$FC + 1
    t.$FA = t.$FA - 0.25
    if t.zz then acc = acc + t.zz end
    if i == at then t.zz = 7; t.yy = 1.5; t.$FB = nil end
    if i == at + 3 then t.$FB = 2 end
    if i % $INS == 0 then t["g" .. (i % 7)] = i end
    acc = acc + (t.$FB or 0)
  end
  return acc, t.$FA, t.$FC, t.zz
end
local function r$id()
  for rep = 1, $R do
    for _, at in ipairs{ 1, 2, 5, 1000 } do
      local ok, a, b, c, d = pcall(s$id, $N, at)
      if ok then mix(a); mix(b); mix(c); mix(d) else err(a) end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(3, 20), N = rnd(15, 100), DECL = table.concat(decl, ", "), FA = fa, FB = fb, FC = fc,
      INS = rnd(3, 11) })
end

-- a metatable with __index / __newindex appears mid-loop: set by the loop, by a callee, by a hook function, by a
-- finalizer (the array has holes, the fields are overwritten with nil)
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, at, how)
  local t = {}
  for i = 1, $SZ do t[i] = i end
  t[$HOLE] = nil
  t.x = 1.5; t.y = 2
  local mt = { __index = function(_, k) return k == "x" and 9 or -1 end,
               __newindex = function(tt, k, v) rawset(tt, k, v) end }
  local function setmt() setmetatable(t, mt) end
  local fin = setmetatable({}, { __gc = function() setmetatable(t, mt) end })
  local acc = 0.0
  for i = 1, n do
    if i == at then
      if how == 1 then setmetatable(t, mt)
      elseif how == 2 then setmt()
      elseif how == 3 then debug.sethook(function() setmetatable(t, mt); debug.sethook() end, "", 3)
      else fin = nil; collectgarbage() end
    end
    local v = t[i % ($SZ + 5) + 1]
    acc = acc + (v or 0.5)
    t[i % ($SZ + 2) + 1] = i
    local x = t.x
    acc = acc + (x or 100)
    if i % 3 == 0 then t.x = nil end
    if i % 4 == 0 then t.x = 1.5 end
    t.y = (t.y or 0) + 1
  end
  debug.sethook()
  return acc, t.y, t.x, rawget(t, $HOLE)
end
local function r$id()
  for rep = 1, $R do
    for how = 1, 4 do
      for _, at in ipairs{ 1, 2, 5, 1000 } do
        local ok, a, b, c, d = pcall(s$id, $N, at, how)
        if ok then mix(a); mix(b); mix(c); mix(d) else err(a) end
      end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 6), N = rnd(15, 80), SZ = rnd(8, 30), HOLE = rnd(2, 7) })
end

-- the table local becomes a non-table / another table through debug.setlocal from a callee or a hook function
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, at, v, how)
  local t = { 1, 2, 3, 4, 5, 6, 7, 8, x = 1, y = 2.5 }
  local acc = 0
  for i = 1, n do
    if i == at then
      if how == 1 then (function() setl(2, "t", v) end)()
      else debug.sethook(function() setl(2, "t", v); debug.sethook() end, "", 2) end
    end
    acc = acc + (t[i % 9 + 1] or 0) + (t.x or 1)
    t.y = t.y
  end
  debug.sethook()
  return acc
end
local vs$id = { 5, "abc", {}, { 10, 20, 30, x = 3 }, setmetatable({}, { __index = function(_, k) return 3 end }),
  2.5, true }
local function r$id()
  for rep = 1, $R do
    for _, v in ipairs(vs$id) do
      for how = 1, 2 do
        for _, at in ipairs{ 1, 3, $K } do
          local ok, a = pcall(s$id, $N, at, v, how)
          if ok then mix(a) else err(a) end
        end
      end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 5), N = rnd(15, 80), K = rnd(4, 40) })
end

-- GC barrier: collectable values stored into old (black) tables and fields, weak tables, finalizers, both collectors
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, mode)
  collectgarbage(mode)
  local old, rec = {}, { a = false, b = false }
  for i = 1, 64 do old[i] = i end
  collectgarbage()
  local w = setmetatable({}, { __mode = "v" })
  local fin = 0
  for i = 1, n do
    old[i % 64 + 1] = { i }
    rec.a = { i, { i } }
    rec.b = old[(i * 3) % 64 + 1]
    old[(i * 7) % 64 + 1] = tostring(i) .. "x"
    w[i % 8 + 1] = { i }
    if i % 9 == 0 then old[(i + 1) % 64 + 1] = setmetatable({}, { __gc = function() fin = fin + 1 end }) end
    if i % $ST == 0 then collectgarbage("step", 1) end
    if i % $FULL == 0 then collectgarbage() end
  end
  collectgarbage()
  local s = 0
  for i = 1, 64 do
    local v = old[i]
    s = s + (type(v) == "table" and (v[1] or 0) or (type(v) == "string" and #v or v))
  end
  local c = 0
  for i = 1, 8 do if w[i] then c = c + 1 end end
  collectgarbage("incremental")
  return s, rec.a[2][1], c >= 0, fin > 0
end
local function r$id()
  for rep = 1, $R do
    for _, m in ipairs{ "generational", "incremental" } do
      local ok, a, b, c, d = pcall(s$id, $N, m)
      if ok then mix(a); mix(b); mix(c); mix(d) else err(a) end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 4), N = rnd(40, 200), ST = rnd(2, 9), FULL = rnd(20, 70) })
end

-- tables shared between coroutines, a table shrunk / rehashed / rebuilt by a callee while the loop reads it
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local shared = {}
  for i = 1, $SZ do shared[i] = 0 end
  local function worker(idn, cnt)
    return coroutine.wrap(function()
      for i = 1, cnt do
        shared[i % $SZ + 1] = shared[i % $SZ + 1] + idn
        shared.count = (shared.count or 0) + 1
        if i % 3 == 0 then coroutine.yield() end
      end
      return "done"
    end)
  end
  local ws = { worker(1, n), worker(10, n), worker(100, n) }
  local live = 3
  while live > 0 do
    for k = 1, 3 do
      if ws[k] and ws[k]() == "done" then ws[k] = nil; live = live - 1 end
    end
  end
  local s = 0
  for i = 1, $SZ do s = s + shared[i] end
  -- a callee rebuilding the table that the loop reads
  local t = {}
  for i = 1, $SZ do t[i] = i end
  local acc = 0
  for i = 1, n do
    acc = acc + (t[i % ($SZ + 3) + 1] or 0)
    if i % 7 == 0 then
      (function()
        for j = #t, 1, -1 do t[j] = nil end
        for j = 1, $SZ + i % 5 do t[j] = j * 2 end
      end)()
    end
    if i % 11 == 0 then table.remove(t); t[#t + 1] = i; table.insert(t, 1, -i) end
  end
  return s, shared.count, acc, #t
end
local function r$id()
  for rep = 1, $R do
    local ok, a, b, c, d = pcall(s$id, $N)
    if ok then mix(a); mix(b); mix(c); mix(d) else err(a) end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 5), N = rnd(12, 70), SZ = rnd(5, 30) })
end

-- element / field values change type at iteration K (int -> float -> string -> nil -> table) and arrays of mixed values
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, k, repl)
  local t = {}
  local o = { f = 1, g = 2.5 }
  for i = 1, $SZ do t[i] = i end
  local acc = 0.0
  for i = 1, n do
    if i == k then t[i % $SZ + 1] = repl; o.f = repl end
    local v = t[i % $SZ + 1]
    acc = acc + v * 2
    acc = acc + o.f + o.g
    t[(i + 1) % $SZ + 1] = i * 0.5
    o.g = o.g + 1
    if i == k + 3 then t[i % $SZ + 1] = i; o.f = 4 end
  end
  return acc
end
local rs$id = { "10", 7, 0.5, "x", false, {}, 1 << 62, -0.0 }
local function r$id()
  for rep = 1, $R do
    for _, r in ipairs(rs$id) do
      for _, k in ipairs{ 1, 2, 5, $K } do
        local ok, a = pcall(s$id, $N, k, r)
        if ok then mix(a) else err(a) end
      end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(2, 10), N = rnd(12, 90), SZ = rnd(5, 25), K = rnd(6, 60) })
end

-- numeric kernels: matrix product, n-body style field updates, sieve and permutations over tables
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local a, b = {}, {}
  for i = 1, n do
    local ra, rb = {}, {}
    for j = 1, n do ra[j] = (i + j) / n; rb[j] = (i * j % 7) / n end
    a[i], b[i] = ra, rb
  end
  local tr = 0.0
  for i = 1, n do
    local ai = a[i]
    for j = 1, n do
      local s = 0.0
      for k = 1, n do s = s + ai[k] * b[k][j] end
      if i == j then tr = tr + s end
    end
  end
  local bodies = { { x = 0, vx = 0.5, m = 2.0 }, { x = 1.5, vx = -0.25, m = 1.0 }, { x = -2.0, vx = 0.125, m = 0.5 } }
  for step = 1, n * 4 do
    for i = 1, #bodies do
      local bi = bodies[i]
      local vx = bi.vx
      for j = i + 1, #bodies do
        local bj = bodies[j]
        local d = bi.x - bj.x
        local mag = 0.01 / (d * d + 1)
        vx = vx - d * bj.m * mag
        bj.vx = bj.vx + d * bi.m * mag
      end
      bi.vx = vx
      bi.x = bi.x + 0.01 * vx
    end
  end
  local flags = {}
  for i = 2, n * 8 do flags[i] = true end
  for i = 2, n * 8 do
    if flags[i] then for j = i * i, n * 8, i do flags[j] = false end end
  end
  local c = 0
  for i = 2, n * 8 do if flags[i] then c = c + 1 end end
  return tr, bodies[1].x, bodies[3].vx, c
end
local function r$id()
  for rep = 1, $R do
    local ok, a, b, c, d = pcall(s$id, $N)
    if ok then mix(a); mix(b); mix(c); mix(d) else err(a) end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 6), N = rnd(4, 14) })
end

------------------------------------------------------------------ T7: policy (respecialisation, thrash, give-up)

-- Live-in type flips between calls: the speculation of a region's entry fails until the policy retargets and widens it.
-- K = 1, 7, 1000 calls per flip; live-in kinds: number (int/float), numeric string, table (array/hash), key (int/float/
-- subnormal float/string), element types of an array.
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, v, t, k)
  local acc = v
  for i = 1, n do
    acc = acc + i
    local e = t[k]
    acc = acc + (e or 0)
    if i % 5 == 0 then t[k] = (e or 0) + 1 end
  end
  return acc
end
local function r$id()
  local arr = {}
  for i = 1, 24 do arr[i] = i end
  local hash = { a = 1, b = 2.5, [3] = 7 }
  for _, K in ipairs{ 1, 7, 1000 } do
    for c = 1, $C do
      local flip = (c // K) % 2 == 0
      local v, t, k
      local sel = (c // K) % $MODES
      if sel == 0 then v, t, k = 1, arr, 3
      elseif sel == 1 then v, t, k = 0.5, arr, 3
      elseif sel == 2 then v, t, k = "7", arr, 3.0
      elseif sel == 3 then v, t, k = 2, hash, "a"
      elseif sel == 4 then v, t, k = 2^53, arr, 5e-324
      elseif sel == 5 then v, t, k = 3, 5, 1
      else v, t, k = -0.0, hash, 3 end
      if not flip and K == 1 then v = v + 0 end
      local ok, a = pcall(s$id, $N, v, t, k)
      if ok then mix(a) else err(a) end
    end
  end
  for i = 1, 24 do mix(arr[i]) end
  mix(hash.a); mix(hash[3])
end
run("r$id", r$id)
]], { id = id, C = N(40, 200), N = rnd(10, 40), MODES = rnd(2, 7) })
end

-- Thrash and give-up: four live-ins change type one after the other (every phase invalidates the speculation of the
-- previous version), and an element-type flip of an array read by the loop at several periods.
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n, a, b, c, d, t)
  local acc = 0
  for i = 1, n do
    acc = acc + a + b + c + d
    acc = acc + t[i % #t + 1]
  end
  return acc
end
local function r$id()
  local vals = { 1, 2, 3, 4 }
  local t = {}
  for i = 1, 12 do t[i] = i end
  for phase = 0, 5 do
    for call = 1, $C do
      if call % $P == 0 then
        for i = 1, #t do t[i] = (math.type(t[i]) == "integer") and t[i] + 0.5 or math.floor(t[i]) end
      end
      local ok, a = pcall(s$id, $N, vals[1], vals[2], vals[3], vals[4], t)
      if ok then mix(a) else err(a) end
    end
    local w = phase % 4 + 1
    vals[w] = (math.type(vals[w]) == "integer") and vals[w] + 0.25 or math.floor(vals[w])
  end
end
run("r$id", r$id)
]], { id = id, C = N(20, 80), N = rnd(8, 30), P = rnd(1, 9) })
end

------------------------------------------------------------------ T6: calls and intrinsics in regions
-- every inline math function on special operands; the callee is a parameter, so one call site sees several functions
T[#T + 1] = function(id)
  return tpl([[
local fns$id = { math.sqrt, math.floor, math.ceil, math.abs, math.sin, math.cos, math.exp, math.log, math.min, math.max }
local vals$id = { 0, 1, -1, 7, 1 << 62, math.mininteger, math.maxinteger, 0.0, -0.0, 0.5, -0.5, 1.5, -1.5, 2.5, 1e15, -1e15,
  2^53, 2^62, 2^63, -2^63, 1e300, 1/0, -1/0, 0/0, 3.0, 1e-300 }
local function s$id(n, x, y, f)
  local r1, r2, r3, r4, r5, r6
  for i = 1, n do
    r1 = f(x)
    r2 = math.min(x, y)
    r3 = math.max(y, x)
    r4 = math.floor(x)
    r5 = math.abs(y)
    r6 = math.sqrt(x)
  end
  return r1, r2, r3, r4, r5, r6
end
local function s2$id(n, x, y, f)
  local r1, r2
  for i = 1, n do r1 = f(x, y); r2 = f(y, x) end
  return r1, r2
end
local function r$id()
  for rep = 1, $R do
    for xi = 1, #vals$id do
      local x, y = vals$id[xi], vals$id[(xi * $MUL) % #vals$id + 1]
      for fi = 1, #fns$id do
        local ok, a, b, c, d, e, g = pcall(s$id, $N, x, y, fns$id[fi])
        if ok then mix(a); mix(b); mix(c); mix(d); mix(e); mix(g) else err(a) end
        local ok2, a2, b2 = pcall(s2$id, $N, x, y, fns$id[fi])
        if ok2 then mix(a2); mix(b2) else err(a2) end
      end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 3), N = rnd(4, 12), MUL = pick{ 1, 3, 5, 7, 11 } })
end

-- the callee changes under the loop: an upvalue rebound, a library field replaced, from the body and from a callee
T[#T + 1] = function(id)
  return tpl([[
local f$id = math.sqrt
local function rebind$id(k) if k == 1 then f$id = function(x) return x * 2 end elseif k == 2 then f$id = math.abs else f$id = math.sqrt end end
local function s$id(n, at, k)
  local s = 0.0
  for i = 1, n do
    if i == at then rebind$id(k) end
    s = s + f$id(i + 0.5)
  end
  return s
end
local function h$id(n, at, repl)
  local s = 0.0
  local save = math.floor
  for i = 1, n do
    if i == at then math.floor = repl end
    s = s + math.floor(i + 0.25)
  end
  math.floor = save
  return s
end
local function r$id()
  for rep = 1, $R do
    for _, at in ipairs{ 1, $AT, 1000000 } do
      for k = 0, 2 do
        f$id = math.sqrt
        local ok, a = pcall(s$id, $N, at, k)
        if ok then mix(a) else err(a) end
      end
      local ok, a = pcall(h$id, $N, at, function(x) return -x end)
      if ok then mix(a) else err(a) end
      local ok2, a2 = pcall(h$id, $N, at, 5)
      if ok2 then mix(a2) else err(a2) end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 4), N = rnd(15, 60), AT = rnd(3, 14) })
end

-- hooks installed in the middle of a loop of inline calls: the interpreter must see every call, count and line
T[#T + 1] = function(id)
  return tpl([[
local hk$id = 0
local function s$id(n, at, kind)
  local s = 0.0
  for i = 1, n do
    if i == at then
      if kind == 1 then debug.sethook(function() hk$id = hk$id + 1 end, "c")
      elseif kind == 2 then debug.sethook(function() hk$id = hk$id + 1 end, "", $CNT)
      else debug.sethook(function() hk$id = hk$id + 1 end, "l") end
    end
    s = s + math.sqrt(i + 0.5) + math.floor(s)
  end
  debug.sethook()
  rehook()
  return s
end
local function r$id()
  for rep = 1, $R do
    for kind = 1, 3 do
      local ok, a = pcall(s$id, $N, $AT, kind)
      if ok then mix(a) else err(a) end
    end
    mix(hk$id)
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 4), N = rnd(15, 60), AT = rnd(2, 12), CNT = rnd(1, 9) })
end

-- operands that change type: strings (coerced), nil, tables, booleans; errors carry the function name and argument
T[#T + 1] = function(id)
  local vs = { "'4'", "'0x10'", "' 7 '", "'abc'", "true", "{}", "9", "-3", "2^53", "0/0", "nil", "1.5" }
  return tpl([[
local function s$id(n, at, v)
  local s = 0.0
  local x = 2.5
  for i = 1, n do
    if i == at then x = v end
    s = s + math.sqrt(x) + math.floor(x) + math.abs(x) + math.min(x, 4.0) + math.max(1, x)
  end
  return s
end
local vs$id = { $VS }
local function r$id()
  for rep = 1, $R do
    for vi = 1, #vs$id + 1 do
      for _, at in ipairs{ 2, $AT } do
        local ok, a = pcall(s$id, $N, at, vs$id[vi])
        if ok then mix(a) else err(a) end
      end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 3), N = rnd(10, 40), AT = rnd(3, 25), VS = table.concat(vs, ", ") })
end

-- Lua callees whose results feed typed arithmetic: float / integer / string / nil results, the type changing at K
T[#T + 1] = function(id)
  return tpl([[
local mode$id = 0
local function g$id(x)
  if mode$id == 0 then return x * 0.5
  elseif mode$id == 1 then return x + 1
  elseif mode$id == 2 then return "2.5"
  elseif mode$id == 3 then return nil
  else return x // 1 end
end
local function s$id(n, at, m2)
  local a, b = 0, 0.0
  for i = 1, n do
    if i == at then mode$id = m2 end
    local r = g$id(i)
    a = a + i
    b = b + r * 2
  end
  mode$id = 0
  return a, b
end
local function r$id()
  for rep = 1, $R do
    for m2 = 0, 4 do
      for _, at in ipairs{ 1, $AT, 1000000 } do
        local ok, a, b = pcall(s$id, $N, at, m2)
        if ok then mix(a); mix(b) else err(a) end
      end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 3), N = rnd(15, 50), AT = rnd(3, 14) })
end

-- generic loop with intrinsics, nested loops, coroutines yielding in the body
T[#T + 1] = function(id)
  return tpl([[
local function s$id(n)
  local co = coroutine.wrap(function(m)
    local acc = 0.0
    for i = 1, m do
      acc = acc + math.sqrt(i) + math.ceil(acc * 0.25) + math.abs(-i) + math.min(i, 3) + math.max(acc, 1.5)
      if i % $Y == 0 then coroutine.yield(acc) end
    end
    return -acc
  end)
  local v = co(n)
  local c = 0
  while v >= 0 do c = c + v; v = co() end
  return c, v
end
local function r$id()
  for rep = 1, $R do
    local ok, a, b = pcall(s$id, $N)
    if ok then mix(a); mix(b) else err(a) end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 4), N = rnd(20, 90), Y = rnd(2, 9) })
end

-- literal operands (typed facts at the call): chunks compiled with `load`, one per (function, operand) pair
T[#T + 1] = function(id)
  return tpl([[
local lits$id = { "0", "1", "-1", "7", "(1<<62)", "math.mininteger", "math.maxinteger", "0.0", "(-0.0)", "0.5", "-0.5", "1.5", "-1.5",
  "2.5", "1e15", "2^53", "2^62", "2^63", "-2^63", "1e300", "(1/0)", "(-1/0)", "(0/0)", "3.0", "1e-300" }
local fn1$id = { "sqrt", "floor", "ceil", "abs", "sin", "cos", "exp", "log" }
local function r$id()
  for rep = 1, $R do
    for li = 1, #lits$id do
      local f1 = fn1$id[(li * $M1 + rep) % #fn1$id + 1]
      local l2 = lits$id[(li * $M2 + rep) % #lits$id + 1]
      local src = "local mix, err = ...; local x, y = " .. lits$id[li] .. ", " .. l2 ..
        "; local r, q for i = 1, $N do r = math." .. f1 .. "(x); q = math.min(x, y); mix(r); mix(q); mix(math.max(y, x)); mix(r + 1) end"
      local f = load(src)
      local ok, e = pcall(f, mix, err)
      if not ok then err(e) end
    end
  end
end
run("r$id", r$id)
]], { id = id, R = N(1, 3), N = rnd(6, 20), M1 = pick{ 1, 3, 5, 7 }, M2 = pick{ 2, 3, 5, 11 } })
end

------------------------------------------------------------------ prototype-shaped templates (template set "tr", P8b)
-- Programs of the shape the trace compiler records: numeric for/while loops inside local functions over int/float
-- arrays without metatables, upvalue accumulators, small local callees, math intrinsics, loop-carried recurrences,
-- ragged arrays, step variations and type changes after iteration K. Selected with gen.lua arg 4 = "tr" (about 75% of
-- the snippets of a seed come from here, the rest from T); without the argument the stream of T is unchanged.
local TT = {}
local SINKT = {} -- indices of the allocation-sinking templates (JITDIFF_TSET=sink)
local function ARR(kind) -- Lua expression building an array of n elements (function of n)
  if kind == 1 then return "function(n) local t = {} for i = 1, n do t[i] = (i * 7 + 3) % 31 - 9 end return t end"
  elseif kind == 2 then return "function(n) local t = {} for i = 1, n do t[i] = (i % 13) * 0.25 - 1.5 end return t end"
  elseif kind == 3 then return "function(n) local t = {} for i = 1, n do t[i] = (i % 5 == 0) and i * 0.5 or i end return t end"
  elseif kind == 4 then return "function(n) local t = {} for i = 1, n do t[i] = i * 1000003 * 1000003 end return t end"
  else return "function(n) local t = {} for i = 1, n do t[i] = (i % 7) + 0.0 end return t end" end
end

-- sum / product-ish loops with an upvalue accumulator, step variations
TT[#TT + 1] = function(id)
  return tpl([[
local mk$id = $ARR
local up$id = 0
local function s$id(t, n, c)
  local s = $S0
  for i = $FROM do
    s = s $OP t[i] * c
    up$id = up$id + 1
  end
  return s, up$id
end
local function w$id(t, n, c)
  local s, i = $S0, 1
  while i <= n do
    s = s $OP t[i] + c
    i = i + $STEP
  end
  return s, i
end
local function r$id()
  local tn = mk$id($N)
  for _, c in ipairs{ 1, 2, 0.5, -3, 1.0 } do
    local a, b = s$id(tn, $N, c); mix(a); mix(b)
    a, b = w$id(tn, $N, c); mix(a); mix(b)
  end
  tn[$K] = $FLIP
  local ok, a, b = pcall(s$id, tn, $N, 2); if ok then mix(a); mix(b) else err(a) end
  ok, a, b = pcall(s$id, tn, $N, 2); if ok then mix(a); mix(b) else err(a) end
end
run("r$id", r$id)
]], { id = id, ARR = ARR(rnd(5)), N = N(150, 900), K = rnd(5, 140), S0 = pick{ "0", "0.0", "1", "7" },
    OP = pick{ "+", "-", "+", "+" }, STEP = pick{ 1, 1, 2, 3 },
    FLIP = pick{ "0.5", "'3'", "2^53", "math.maxinteger", "-0.0", "1e300" },
    FROM = pick{ "1, n", "1, n, 2", "n, 1, -1", "n, 1, -3", "2, n - 1", "1, n, 5" } })
end

-- invariant math.sqrt from an upvalue, float/int mixes
TT[#TT + 1] = function(id)
  return tpl([[
local mk$id = $ARR
local u$id = $U
local function s$id(t, n)
  local s = 0.0
  for i = 1, n do
    local k = math.sqrt(u$id)
    s = s + t[i] * k
  end
  return s
end
local function q$id(t, n)
  local s = 0
  for i = 1, n do
    s = s + math.floor(t[i] * 0.5) + math.ceil(t[i] / 3) + math.abs(t[i])
  end
  return s
end
local function r$id()
  local t = mk$id($N)
  for _, u in ipairs{ 4, 2.25, 0, -1, 1e300, 9 } do
    u$id = u
    mix(s$id(t, $N))
    local ok, e = pcall(q$id, t, $N); if ok then mix(e) else err(e) end
  end
  t[$N // 2] = 'x'
  local ok, e = pcall(s$id, t, $N); if ok then mix(e) else err(e) end
  ok, e = pcall(q$id, t, $N); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, ARR = ARR(rnd(5)), N = N(100, 700), U = pick{ "16", "2.0", "49", "0.0" } })
end

-- loop-carried recurrences t[i] = t[i-k]*c + x, ragged arrays
TT[#TT + 1] = function(id)
  return tpl([[
local function rec$id(t, n, c, x)
  for i = $KK + 1, n do t[i] = t[i - $KK] * c + x end
  return t[n]
end
local function sm$id(t, n)
  local s = 0
  for i = 1, n do s = s + (t[i] or 0) end
  return s
end
local function r$id()
  for _, cfg in ipairs{ { 1, 0 }, { 2, 1 }, { 1.5, 0.25 }, { -1, 3 }, { 3, 0.5 } } do
    local t = {}
    for i = 1, $KK do t[i] = i + 0.5 * ($KK - i) end
    local n = $N
    mix(rec$id(t, n, cfg[1], cfg[2]))
    mix(sm$id(t, n))
    local u = { }
    for i = 1, $KK do u[i] = i end
    local ok, e = pcall(rec$id, u, n, cfg[1], cfg[2])
    if ok then mix(e) else err(e) end
    local v = { }
    for i = 1, $KK + 3 do v[i] = i end
    v[$KK + 2] = nil
    ok, e = pcall(rec$id, v, n, cfg[1], cfg[2])
    if ok then mix(e) else err(e) end
    mix(#u)
  end
end
run("r$id", r$id)
]], { id = id, N = N(80, 600), KK = pick{ 1, 1, 2, 3 } })
end

-- may-alias array pairs
TT[#TT + 1] = function(id)
  return tpl([[
local function cp$id(a, b, n)
  for i = 2, n do a[i] = b[i - 1] + $D end
  return a[n]
end
local function ac$id(a, b, n)
  local s = 0
  for i = 1, n do
    a[i] = a[i] + 1
    s = s + b[i]
  end
  return s
end
local function r$id()
  local n = $N
  local a, b = {}, {}
  for i = 1, n do a[i] = i; b[i] = i * 0.5 end
  mix(cp$id(a, a, n)); mix(cp$id(a, b, n)); mix(cp$id(b, a, n)); mix(cp$id(b, b, n))
  mix(ac$id(a, a, n)); mix(ac$id(a, b, n)); mix(ac$id(b, b, n)); mix(ac$id(a, a, n))
  mix(a[n]); mix(b[n])
end
run("r$id", r$id)
]], { id = id, N = N(100, 800), D = pick{ "1", "0.5", "2", "-1" } })
end

-- calls to small local functions (1-4 levels), callee changes at iteration K
TT[#TT + 1] = function(id)
  return tpl([[
local function f1$id(x) return x * 2 + 1 end
local function f2$id(x) return f1$id(x) + f1$id(x + 1) end
local function f3$id(x, y) return f2$id(x) - y, y end
local function f4$id(x) local a, b = f3$id(x, 3) return a * b end
local cal$id = f1$id
local function rebind$id(k)
  if k == 1 then cal$id = function(x) return x - 1 end
  elseif k == 2 then cal$id = math.abs
  elseif k == 3 then cal$id = f1$id end
end
local function s$id(n, flipat)
  local s = 0
  for i = 1, n do
    if i == flipat then rebind$id($RB) end
    s = s + f4$id(i) + cal$id(i) + f2$id(i % 7)
  end
  return s
end
local function t$id(n)
  local s, k = 0, 0.5
  for i = 1, n do
    local a, b = f3$id(i, k)
    s = s + a + b
    k = k + 0.25
  end
  return s
end
local function r$id()
  mix(s$id($N, 0)); mix(s$id($N, $K)); mix(t$id($N)); mix(s$id($N, $K)); rebind$id(3)
  f1$id = function(x) return x * 3 end
  mix(s$id($N, 0)); mix(t$id($N))
end
run("r$id", r$id)
]], { id = id, N = N(80, 500), K = rnd(5, 60), RB = rnd(3) })
end

-- intrinsics with special values
TT[#TT + 1] = function(id)
  return tpl([[
local vals$id = { 0, 1, -1, 2, 0.5, -0.5, 1e15, 2^53, 3.0, -3.5, 1e300, math.maxinteger, math.mininteger, 1/0, -1/0, 0/0, -0.0 }
local function s$id(n, x, y)
  local r1, r2, r3, r4, r5, r6 = 0, 0, 0, 0, 0, 0
  for i = 1, n do
    r1 = math.sqrt(x)
    r2 = math.min(x, y, i)
    r3 = math.max(y, x)
    r4 = math.floor(x)
    r5 = math.ceil(y)
    r6 = math.abs(x - y)
  end
  return r1, r2, r3, r4, r5, r6
end
local function r$id()
  for xi = 1, #vals$id do
    local ok, a, b, c, d, e, f = pcall(s$id, $N, vals$id[xi], vals$id[(xi * $M) % #vals$id + 1])
    if ok then mix(a); mix(b); mix(c); mix(d); mix(e); mix(f) else err(a) end
  end
  local sq = math.sqrt
  math.sqrt = function(x) return x end
  mix(s$id($N, 4, 2))
  math.sqrt = sq
  mix(s$id($N, 4, 2))
end
run("r$id", r$id)
]], { id = id, N = N(30, 200), M = pick{ 1, 3, 5, 7 } })
end

-- while/repeat with rare branches, break and early exit
TT[#TT + 1] = function(id)
  return tpl([[
local function w$id(t, n, lim)
  local i, s, hits = 1, 0, 0
  while i <= n do
    local v = t[i]
    if v > lim then hits = hits + 1; s = s - v elseif v < -lim then s = s + v * 2 else s = s + v end
    if hits > $H then break end
    i = i + 1
  end
  return s, hits, i
end
local function rp$id(n, c)
  local i, s = 0, 0
  repeat
    i = i + 1
    s = s + (i % c == 0 and i or 1)
  until i >= n or s > $BIG
  return s, i
end
local function r$id()
  local t = {}
  for i = 1, $N do t[i] = (i * 37) % 101 - 50 end
  for _, lim in ipairs{ 5, 40, 49, 100 } do
    local a, b, c = w$id(t, $N, lim); mix(a); mix(b); mix(c)
  end
  t[$N - 1] = 0.5
  local a, b, c = w$id(t, $N, 20); mix(a); mix(b); mix(c)
  for c = 1, 6 do local a, b = rp$id($N, c); mix(a); mix(b) end
end
run("r$id", r$id)
]], { id = id, N = N(60, 600), H = pick{ 3, 10, 100000 }, BIG = pick{ 1000, 100000, 100000000 } })
end

-- type-flip loops (accumulator / element type changes after iteration K)
TT[#TT + 1] = function(id)
  return tpl([[
local function s$id(n, k)
  local acc, x = 0, 1
  for i = 1, n do
    if i == k then acc = acc + 0.5; x = x + $XF end
    acc = acc + x * i
    x = x + 1
  end
  return acc, x
end
local function e$id(t, n)
  local s = 0
  for i = 1, n do s = s + t[i] end
  return s
end
local function r$id()
  mix(s$id($N, $N + 1)); mix(s$id($N, $K)); mix(s$id($N, 1))
  for _, v in ipairs{ 1.5, 2^53, 'x', false, math.maxinteger } do
    local t = {}
    for i = 1, $N do t[i] = i end
    t[$K] = v
    local ok, e = pcall(e$id, t, $N)
    if ok then mix(e) else err(e) end
    t[$K] = $K
    ok, e = pcall(e$id, t, $N)
    if ok then mix(e) else err(e) end
  end
end
run("r$id", r$id)
]], { id = id, N = N(80, 500), K = rnd(3, 70), XF = pick{ "0.5", "1", "2^62", "0.0" } })
end

-- nested loops with inner trip counts 0..12
TT[#TT + 1] = function(id)
  return tpl([[
local function s$id(n, m)
  local s = 0
  for i = 1, n do
    for j = 1, (i * $M) % m do
      s = s + i * j
    end
    s = s ~ i
  end
  return s
end
local function g$id(t, rows, cols)
  local s = 0.0
  for i = 1, rows do
    local row = t[i]
    for j = 1, cols do s = s + row[j] * (j - i) end
  end
  return s
end
local function r$id()
  for m = 1, 13 do mix(s$id($N, m)) end
  local t = {}
  for i = 1, 12 do local row = {} for j = 1, 12 do row[j] = i + j * 0.5 end t[i] = row end
  for rows = 0, 12, 3 do mix(g$id(t, rows, 12 - rows % 5)) end
  t[5][7] = 3
  mix(g$id(t, 12, 12))
  t[6][3] = nil
  local ok, e = pcall(g$id, t, 12, 12); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(40, 300), M = pick{ 1, 3, 5, 7 } })
end

-- float for loops, int/float start and step mixes
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(a, b, st)
  local s, c = 0.0, 0
  for x = a, b, st do s = s + x * x; c = c + 1; if c > $LIM then break end end
  return s, c
end
local function r$id()
  mix(f$id(0.0, $N, 0.25)); mix(f$id(1, $N, 0.5)); mix(f$id(0, 10, 3)); mix(f$id($N, 0, -1.5))
  mix(f$id(0.1, 2, 0.1)); mix(f$id(math.maxinteger - 5, math.maxinteger, 1)); mix(f$id(1, 3.5, 1))
  local ok, e = pcall(f$id, 1, 10, 0); if ok then mix(e) else err(e) end
  ok, e = pcall(f$id, 1, 'x', 1); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(30, 300), LIM = pick{ 50, 400, 100000 } })
end

-- array fill / append / swap (bubble pass) / table write loops
TT[#TT + 1] = function(id)
  return tpl([[
local function fill$id(n, c)
  local t = {}
  for i = 1, n do t[i] = i * c end
  return t
end
local function app$id(n)
  local t = {}
  for i = 1, n do t[#t + 1] = i % 3 == 0 and i * 0.5 or i end
  return t
end
local function bub$id(t, n)
  local sw = 0
  for j = 1, n - 1 do
    if t[j] > t[j + 1] then t[j], t[j + 1] = t[j + 1], t[j]; sw = sw + 1 end
  end
  return sw
end
local function r$id()
  local a, b = fill$id($N, 3), fill$id($N, 0.5)
  mix(#a); mix(#b); mix(a[$N]); mix(b[$N])
  local t = app$id($N)
  mix(#t); mix(t[$N])
  local p = {}
  for i = 1, $N do p[i] = (i * 53) % 97 end
  for _ = 1, 6 do mix(bub$id(p, $N)) end
  mix(p[1]); mix(p[$N])
  p[$N // 2] = 0.5
  for _ = 1, 3 do mix(bub$id(p, $N)) end
  p[$N // 3] = 'q'
  local ok, e = pcall(bub$id, p, $N); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(60, 500) })
end

-- rare branches in a hot loop (side traces), counters in upvalues written by inlined closures
TT[#TT + 1] = function(id)
  return tpl([[
local ca$id, cb$id = 0, 0
local function bump$id() ca$id = ca$id + 1 end
local function s$id(t, n)
  local s = 0
  for i = 1, n do
    local v = t[i]
    if v % $P == 0 then bump$id(); s = s + v
    elseif v % $Q == 1 then cb$id = cb$id + v
    else s = s - 1 end
  end
  return s
end
local function r$id()
  local t = {}
  for i = 1, $N do t[i] = (i * 17 + 5) % 211 end
  for _ = 1, 4 do mix(s$id(t, $N)); mix(ca$id); mix(cb$id) end
  t[$N // 2] = 6.0
  mix(s$id(t, $N)); mix(ca$id); mix(cb$id)
  t[$N // 2] = 6.5
  local ok, e = pcall(s$id, t, $N); if ok then mix(e) else err(e) end
  mix(ca$id)
end
run("r$id", r$id)
]], { id = id, N = N(80, 600), P = pick{ 3, 5, 11, 64 }, Q = pick{ 2, 7, 13 } })
end

------------------------------------------------------------------ assembly
local out = {}
local function emit(s) out[#out + 1] = s end

emit([[
-- generated by tests/jitdiff/gen.lua, seed ]] .. seed .. (hookmode and (" hook=" .. hookmode) or "") .. [[

local H = 0
local HC, HL, HLH = 0, 0, 0
local HF, HMASK, HCNT
local mtype, sfmt, sbyte = math.type, string.format, string.byte
local function mix(v)
  local t = mtype(v)
  local x
  if t == "integer" then x = v
  elseif t == "float" then
    if v ~= v then x = 77 elseif v == math.huge then x = 78 elseif v == -math.huge then x = 79
    else
      local s = sfmt("%.17g", v)
      x = 5
      for i = 1, #s do x = x * 31 + sbyte(s, i) end
    end
    x = x * 3 + 1
  elseif type(v) == "string" then
    x = #v
    for i = 1, #v do x = x * 31 + sbyte(v, i) end
  elseif v == nil then x = 1
  elseif v == true then x = 2
  elseif v == false then x = 3
  else x = #type(v) * 1000003 end
  H = (H ~ x) * 1099511628211 + 0x9E3779B9
  H = H ~ (H >> 29)
end
local errs = {}
local function err(e)
  local s
  local te = type(e)
  if te == "string" then s = e
  elseif te == "table" or te == "function" or te == "thread" or te == "userdata" then s = "<" .. te .. ">"
  else s = tostring(e) end
  errs[s] = (errs[s] or 0) + 1
  mix(s)
end
local function flusherrs(name)
  local ks = {}
  for k in pairs(errs) do ks[#ks + 1] = k end
  table.sort(ks)
  for i, k in ipairs(ks) do
    if i <= 12 then print(name, "ERRMSG", errs[k], k) end
  end
  if #ks > 12 then print(name, "ERRMSG", "... " .. (#ks - 12) .. " more distinct") end
  errs = {}
end
-- (re)installs the prelude hook of the hook variants (or removes every hook in the plain variant); used by snippets
-- that install their own hook in the middle of a run and remove it again
local function rehook()
  if HF then debug.sethook(HF, HMASK, HCNT) else debug.sethook() end
end
-- setl(lvl, name, v): debug.setlocal on the local `name` of the function `lvl` levels above the caller of setl
-- (lvl 1 = the caller itself, 2 = its caller: a callee or hook function pokes the loop that called it)
local function setl(lvl, name, v)
  lvl = lvl + 1
  for i = 1, 60 do
    local nm = debug.getlocal(lvl, i)
    if nm == nil then return false end
    if nm == name then debug.setlocal(lvl, i, v); return true end
  end
  return false
end
local RUNIDX = 0
local function run(name, f, ...)
  -- hook variants: the prelude hook is active for every other snippet; the snippets in between run without a hook,
  -- so that compiled code is entered (it is never entered while a hook is installed)
  RUNIDX = RUNIDX + 1
  if HF then if RUNIDX % 2 == 0 then debug.sethook() else rehook() end end
  local r = table.pack(pcall(f, ...))
  if r[1] then for i = 2, r.n do mix(r[i]) end else err(r[2]) end
  flusherrs(name)
  if HOOKS then print(name, H, HC, HL, HLH) else print(name, H) end
end
]])
-- fix: HOOKS must be a local visible to run(); define before
out[#out] = out[#out]:gsub("local H = 0\n", "local H = 0\nlocal HOOKS = " .. tostring(hookmode ~= nil) .. "\n", 1)

if hookmode then
  local mask = ({ count = '""', line = '"l"', both = '"l"' })[hookmode]
  local cnt = ({ count = tostring(rnd(1, 400)), line = "0", both = tostring(rnd(50, 3000)) })[hookmode]
  emit(tpl([[
local getinfo = debug.getinfo
HF = function(ev, line)
  if ev == "count" then
    HC = HC + 1
    HLH = (HLH * 31 + getinfo(2, "l").currentline) & 0xffffffffff
  else
    HL = HL + 1
    HLH = (HLH * 31 + line) & 0xffffffffff
  end
end
HMASK, HCNT = $MASK, $CNT
debug.sethook(HF, HMASK, HCNT)
]], { MASK = mask, CNT = cnt }))
end

------------------------------------------------------------------ field-heavy templates of the "tr" set (task F1)
-- hash-part accesses: object fields of every type, globals (the _ENV upvalue), SELF on plain tables, nil fields,
-- keys that are absent / inserted / removed, metatables with and without __index / __newindex, node-layout variations.

-- object fields: reads, read-modify-write, types that change after iteration K, extra fields, field order
TT[#TT + 1] = function(id)
  return tpl([[
local function mk$id(kind)
  local o
  if kind == 1 then o = { x = 1, y = 2.5, n = 0, s = "a" }
  elseif kind == 2 then o = { s = "a", n = 0, y = 2.5, x = 1 }
  elseif kind == 3 then o = { x = 1, y = 2.5, n = 0, s = "a", p = 1, q = 2, r = 3, t = 4, u = 5 }
  else o = { x = 1 }; o.y = 2.5; o.n = 0; o.s = "a" end
  return o
end
local function f$id(o, n)
  local a = 0
  for i = 1, n do
    o.x = o.x + $DX
    o.y = o.y * $C + o.n
    o.n = o.n + 1
    a = a + o.x
    if i == $K then o.$FLD = $FLIP end
  end
  return a, o.x, o.y, o.n
end
local function r$id()
  for kind = 1, 4 do
    local o = mk$id(kind)
    local a, x, y, n = f$id(o, $N)
    mix(a); mix(x); mix(y); mix(n)
    local ok, e = pcall(f$id, o, $N)
    if ok then mix(e) else err(e) end
    ok, e = pcall(f$id, o, $N)
    if ok then mix(e) else err(e) end
  end
end
run("r$id", r$id)
]], { id = id, N = N(80, 500), K = rnd(5, 70), DX = pick{ 1, 2, 3, "0.5" }, C = pick{ "1.0000001", "0.999", "1" },
    FLD = pick{ "x", "y", "n" }, FLIP = pick{ "0.5", "'3'", "2^53", "math.maxinteger", "nil", "true", "{}" } })
end

-- globals through _ENV, a global function inlined, a replaced environment, a global removed in the loop
TT[#TT + 1] = function(id)
  return tpl([[
G$id = 0
GF$id = 0.5
function GA$id(x) G$id = G$id + x end
local function f$id(n)
  for i = 1, n do
    GA$id(i)
    GF$id = GF$id * 1.0001 + 1
    if i == $K then G$id = $FLIP end
  end
  return G$id, GF$id
end
local function e$id(n)
  local env = { acc = 0, add = function(x) return x + 1 end }
  local g = load("local n = ...; for i = 1, n do acc = acc + add(i) end return acc", "e$id", "t", env)
  return g(n)
end
local function r$id()
  mix(f$id($N)); mix(f$id($N))
  G$id = 7
  local ok, e = pcall(f$id, $N); if ok then mix(e) else err(e) end
  mix(e$id($N)); mix(e$id($N))
  GA$id = nil
  ok, e = pcall(f$id, $N); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(60, 400), K = rnd(5, 50), FLIP = pick{ "0.5", "'3'", "2^53", "nil", "true" } })
end

-- SELF on plain tables (method fields), shared and per-object methods, a method replaced mid-run
TT[#TT + 1] = function(id)
  return tpl([[
local function inc$id(self, d) self.n = self.n + d end
local function dbl$id(self, d) self.n = self.n * 2 + d end
local function mk$id(i)
  local o = { n = i, step = 1 }
  o.add = (i % 2 == 0) and inc$id or dbl$id
  o.get = function(self) return self.n end
  return o
end
local function f$id(o, k)
  local s = 0
  for i = 1, k do
    o:add(1)
    s = s + (o:get() % 1000)
    if i == $K then o.add = $REPL end
  end
  return s, o.n
end
local function r$id()
  for i = 1, 6 do
    local o = mk$id(i)
    local s, n = f$id(o, $N)
    mix(s); mix(n)
    local ok, e = pcall(f$id, o, $N)
    if ok then mix(e) else err(e) end
  end
end
run("r$id", r$id)
]], { id = id, N = N(60, 300), K = rnd(5, 50), REPL = pick{ "inc" .. id, "nil", "5", "function(self, d) self.n = -self.n end" } })
end

-- nil fields, absent keys, insertion / removal / rehash of the table in the loop, collector steps
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(t, n)
  local s = 0
  for i = 1, n do
    s = s + (t.a or 1) + (t.b or 10)
    if i % $P == 0 then t.b = nil end
    if i % $Q == 0 then t.b = i end
    if i == $K then t.c = 5; t.d = 6; t.e = 7; t.f = 8; t.g = 9 end
    if i % 19 == 0 then collectgarbage("step", 0) end
  end
  return s, t.a, t.b
end
local function r$id()
  for round = 1, 3 do
    local t = { a = 1, b = 2 }
    mix(f$id(t, $N))
    local u = setmetatable({ a = 1, b = 2 }, round == 2 and { __index = function() return 100 end } or { __gc = false })
    local ok, e = pcall(f$id, u, $N)
    if ok then mix(e) else err(e) end
    local v = setmetatable({ a = 1 }, { __newindex = function(t, k, x) rawset(t, k, x) end })
    ok, e = pcall(f$id, v, $N)
    if ok then mix(e) else err(e) end
  end
end
run("r$id", r$id)
]], { id = id, N = N(60, 300), K = rnd(5, 50), P = pick{ 7, 11, 13 }, Q = pick{ 5, 9, 13 } })
end

-- nested objects and upvalue tables, collectable stores (write barrier), tables swapped in the loop
TT[#TT + 1] = function(id)
  return tpl([[
local cfg$id = { scale = $SC, off = 3, label = "l" }
local function f$id(a, b, n)
  local s = 0
  for i = 1, n do
    s = s + a.v.w * cfg$id.scale + b.v.w + cfg$id.off
    a.v, b.v = b.v, a.v
    cfg$id.label = (i % 2 == 0) and "even" or "odd"
    a.v.w = a.v.w + 1
  end
  return s, a.v.w, b.v.w, cfg$id.label
end
local function r$id()
  local a = { v = { w = 1 } }
  local b = { v = { w = 10 } }
  mix(f$id(a, b, $N))
  collectgarbage("step", 1)
  mix(f$id(a, b, $N))
  b.v = { w = 100 }
  local ok, e = pcall(f$id, a, b, $N); if ok then mix(e) else err(e) end
  a.v = nil
  ok, e = pcall(f$id, a, b, $N); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(40, 250), SC = pick{ 2, 0.5, 3 } })
end

------------------------------------------------------------------ allocation-heavy templates of the "tr" set (task F3)
-- NEWTABLE / SETLIST / key insertion (NEWREF) / LEN on traces: constructors of every shape, insertion into empty and
-- preallocated hash parts (rehash on the way), present / absent / nil-valued keys, __newindex, tables escaping into older
-- tables (barriers), allocation pressure (the collector steps inside the loop: the traces exit before the allocation).

-- array and record constructors, nested constructors, lengths
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(n)
  local s = 0
  for i = 1, n do
    local t = { i, i + $A, i * $B }
    local o = { x = i, y = i * 0.5 }
    local u = { t, { o.x }, k = o }
    s = s + t[1] + t[3] + #t + #u + u[2][1] + u.k.x
    if i % $M == 0 then s = s + o.y end
  end
  return s
end
local function g$id(n)
  local s = 0
  for i = 1, n do
    local t = { i, i, i, i, i, i, i, nil, i }
    local e = {}
    s = s + #e + (t[9] or 0) + (t[8] == nil and 1 or 0)
  end
  return s
end
local function r$id()
  mix(f$id($N)); mix(f$id($N)); mix(f$id(3)); mix(g$id($N)); mix(g$id($N))
  local ok, e = pcall(f$id, $N); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(60, 400), A = rnd(1, 9), B = rnd(2, 5), M = rnd(2, 7) })
end

-- insertion of fields into new tables, interleaved with reads (every insertion may rehash), keys in different orders
TT[#TT + 1] = function(id)
  return tpl([[
local function mk$id(n, kind)
  local s = 0
  for i = 1, n do
    local o = {}
    if kind == 1 then
      o.a = i; s = s + o.a; o.b = i * 2; s = s + o.a + o.b; o.c = i * 3; o.d = i * 4; s = s + o.c + o.d + o.a
    elseif kind == 2 then
      o = { a = i, b = 1 }
      o.c = i; o.d = 2; o.e = 3; s = s + o.a + o.b + o.c + o.d + o.e
    else
      o.z = "s"; o.y = i; o.x = o; o.w = { i }; s = s + o.y + o.x.y + o.w[1] + #o.z
    end
  end
  return s
end
local function r$id()
  for kind = 1, 3 do
    mix(mk$id($N, kind)); mix(mk$id($N, kind)); mix(mk$id(2, kind))
    local ok, e = pcall(mk$id, $N, kind); if ok then mix(e) else err(e) end
  end
end
run("r$id", r$id)
]], { id = id, N = N(60, 400) })
end

-- a key that is absent in the recorded runs and present (with a value or with nil) in later ones, __newindex in the way
TT[#TT + 1] = function(id)
  return tpl([[
local seen$id = 0
local mt$id = { __newindex = function(t, k, v) seen$id = seen$id + 1; rawset(t, k, v) end }
local side$id = {}
local function f$id(o, n)
  local s = 0
  for i = 1, n do
    o.k = i
    o.m = o.k + $C
    s = s + o.m
  end
  return s
end
local function r$id()
  for round = 1, 3 do
    mix(f$id({}, $N)); mix(f$id({}, $N))
    mix(f$id({ k = 1 }, $N))
    local v = { k = 1, m = 2 }; v.k = nil
    mix(f$id(v, $N))
    mix(f$id(setmetatable({}, {}), $N))
    mix(f$id(setmetatable({}, mt$id), $N))
    mix(f$id(setmetatable({}, { __newindex = side$id }), $N))
    mix(f$id(setmetatable({ k = 0 }, mt$id), $N))
  end
  mix(seen$id); mix(side$id.k or -1)
end
run("r$id", r$id)
]], { id = id, N = N(20, 200), C = rnd(0, 5) })
end

-- tables escaping into older tables and upvalues while the collector works (barriers, aging), collection steps between
TT[#TT + 1] = function(id)
  return tpl([[
local keep$id = {}
local last$id
local function f$id(n)
  for i = 1, n do
    local t = { i, w = i }
    local u = { t = t, n = i }
    keep$id[i % $K + 1] = u
    last$id = u
    if i % $P == 0 then u.extra = { t } end
  end
  return last$id.n, last$id.t[1]
end
local function r$id()
  for k = 1, $K + 1 do keep$id[k] = false end
  for round = 1, 4 do
    local a, b = f$id($N)
    mix(a); mix(b)
    collectgarbage("step", $SC)
    a, b = f$id($N)
    mix(a); mix(b)
  end
  local s = 0
  for k = 1, $K + 1 do local u = keep$id[k]; if u then s = s + u.n + u.t[1] + (u.extra and #u.extra or 0) end end
  mix(s)
end
run("r$id", r$id)
]], { id = id, N = N(60, 300), K = rnd(2, 12), P = rnd(2, 5), SC = pick{ 1, 8, 64 } })
end

-- length operator on arrays, strings, tables with holes and with __len, and counters kept in a table
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(t, s, n)
  local r = 0
  for i = 1, n do r = r + #t + #s + i end
  return r
end
local function r$id()
  local a = { 1, 2, 3, 4, 5, 6 }
  local h = { 1, 2, nil, 4 }
  local l = setmetatable({ 1, 2, 3 }, { __len = function() return $LEN end })
  local e = {}
  local big = {}
  for i = 1, 70 do big[i] = i end
  for round = 1, 3 do
    mix(f$id(a, "abc", $N)); mix(f$id(a, string.rep("x", 50), $N))
    mix(f$id(e, "", $N)); mix(f$id(big, "q", $N))
    local ok, v = pcall(f$id, h, "hh", $N); if ok then mix(v) else err(v) end
    ok, v = pcall(f$id, l, "ll", $N); if ok then mix(v) else err(v) end
  end
  a[#a + 1] = 7; a[#a + 1] = 8
  mix(f$id(a, "z", $N))
end
run("r$id", r$id)
]], { id = id, N = N(30, 250), LEN = rnd(0, 9) })
end


-- F3b: CONCAT of strings and numbers (two and three operands), results used as keys and compared
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(n, p, q)
  local t, r = {}, 0
  for i = 1, n do
    local a = p .. i
    local b = i .. q .. p
    local c = (i * 0.5) .. "|" .. i
    t[a] = (t[a] or 0) + i
    r = r + #a + #b + #c
    if a == p .. i then r = r + 1 end
  end
  local k = 0
  for _ in pairs(t) do k = k + 1 end
  return r, k, #(p .. q .. p)
end
local function r$id()
  for round = 1, 3 do
    local a, b, c = f$id($N, "$P", "$Q")
    mix(a); mix(b); mix(c)
  end
  local ok, v = pcall(f$id, $N, "$P", {})
  if ok then mix(v) else err(v) end
  ok, v = pcall(f$id, $N, "", "")
  if ok then mix(v) else err(v) end
  mix(f$id($N, string.rep("w", 38), "ab"))
end
run("r$id", r$id)
]], { id = id, N = N(30, 250), P = pick({ "a", "key", "xyz", "0123456789" }), Q = pick({ "-", "::", "_q" }) })
end

-- F3b: ipairs / pairs / next loops over arrays, hash tables and mixed tables, with stores and deletions
TT[#TT + 1] = function(id)
  return tpl([[
local function s$id(t, n)
  local r = 0
  for round = 1, n do
    for i, v in ipairs(t) do r = r + i + (type(v) == "number" and v or 1) end
    for k, v in pairs(t) do r = r + (type(k) == "number" and k or #k) end
    for k in next, t do r = r + 1 end
  end
  return r
end
local function w$id(t)
  for k, v in pairs(t) do if type(v) == "number" then t[k] = v + $D end end
  for k in pairs(t) do if type(k) == "string" and #k > $L then t[k] = nil end end
end
local function r$id()
  local arr, hash, mixed = {}, {}, {}
  for i = 1, $A do arr[i] = i * 2 end
  for i = 1, $A do hash["h" .. i] = i end
  for i = 1, $A do mixed[i] = i; mixed["m" .. i] = i + 0.5 end
  mixed[true] = 1
  for round = 1, 3 do
    mix(s$id(arr, $N)); mix(s$id(hash, $N)); mix(s$id(mixed, $N)); mix(s$id({}, 3))
    w$id(arr); w$id(hash); w$id(mixed)
  end
  local ok, v = pcall(s$id, nil, 3)
  if ok then mix(v) else err(v) end
  local mt = setmetatable({ 1, 2, 3 }, { __index = function(_, i) if i < 6 then return i end end })
  mix(s$id(mt, 5))
end
run("r$id", r$id)
]], { id = id, N = N(5, 30), A = rnd(5, 70), D = rnd(1, 9), L = rnd(2, 4) })
end

------------------------------------------------------------------ allocation sinking templates of the "tr" set (task S0)
-- temporary tables that do not escape the iteration (sunk), that escape on some iterations (the exit materialises them), that
-- are held by a live local, copied into a second slot, passed to an inlined callee that may exit, with field updates

TT[#TT + 1] = function(id)
  return tpl([[
local keep$id = {}
local function f$id(n)
  local s = 0
  local last$id
  for i = 1, n do
    local v = { x = i, y = i * 0.5, tag = "t" }
    local w = v
    v.x = v.x + $A
    if i % $M == 0 then keep$id[#keep$id % 8 + 1] = w end
    if i % $K == 0 then last$id = v end
    s = s + v.x + w.y + #v.tag
  end
  return s, last$id and last$id.x
end
local function r$id()
  for k = 1, 3 do local a, b = f$id($N); mix(a); mix(b) end
  local a, b = f$id(2); mix(a); mix(b)
  local ok, e = pcall(f$id, $N); if ok then mix(e) else err(e) end
  mix(#keep$id); if keep$id[1] then mix(keep$id[1].x) end
end
run("r$id", r$id)
]], { id = id, N = N(60, 400), A = rnd(1, 9), M = rnd(3, 11), K = rnd(5, 17) })
end

TT[#TT + 1] = function(id)
  return tpl([[
local sink$id
local function mk$id(a, b) local t = { x = a, y = b } return t end
local function len$id(p) local d = p.x * p.x + p.y * p.y if d > $LIM then sink$id = p end return d end
local function f$id(n)
  local s = 0
  local acc = { x = 0, y = 1 }
  for i = 1, n do
    local p = mk$id(i, i + $A)
    local q = mk$id(p.y, p.x)
    s = s + len$id(p) + len$id(q)
    acc = { x = acc.x + 1, y = acc.y + p.x }
    if i % $M == 0 then local nm, val = debug.getlocal(1, 2); mix(val) end
  end
  return s, acc.x, acc.y
end
local function r$id()
  for k = 1, 3 do local a, b, c = f$id($N); mix(a); mix(b); mix(c) end
  mix(sink$id and sink$id.x)
  local ok, e = pcall(f$id, $N); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(60, 400), A = rnd(1, 9), M = rnd(3, 11), LIM = rnd(20000, 90000) })
end

-- F8: GC-observable output. Lines tagged `GCOBS:` (collectgarbage("count"), the result of "step", finalizer progress and weak
-- table contents before a full collection) may differ with allocation sinking on (TR29.2): scripts/jit-diff.sh drops them
-- when either mode of a comparison can sink, and compares them exactly in every other mode (`trnosink` included). What is
-- printed untagged (through `mix`) does not depend on the collector's timing.
TT[#TT + 1] = function(id)
  return tpl([[
local fin$id = 0
local function f$id(n)
  local s = 0
  for i = 1, n do
    local v = { x = i, y = i + 1 }
    if i % $F == 0 then setmetatable({}, { __gc = function() fin$id = fin$id + 1 end }) end
    s = s + v.x * v.y
  end
  return s
end
local function r$id()
  collectgarbage()
  local c0 = collectgarbage("count")
  mix(f$id($N)); mix(f$id($N)); mix(f$id($N))
  print("GCOBS: count-delta", sfmt("%.1f", collectgarbage("count") - c0))
  print("GCOBS: finalized", fin$id)
  local w = setmetatable({}, { __mode = "v" })
  for i = 1, 4 do w[i] = { i } end
  print("GCOBS: weak", w[1] ~= nil, w[4] ~= nil)
  print("GCOBS: step", collectgarbage("step", 1))
  collectgarbage()
  print("GCOBS: count-after-collect", sfmt("%.1f", collectgarbage("count") - c0))
  mix(fin$id)
end
run("r$id", r$id)
]], { id = id, N = N(60, 300), F = rnd(3, 13) })
end

------------------------------------------------------------------ recursion / tail call / multiple-result templates of the "tr" set (task F4)
-- recursive calls inside hot loops (unrolled up to the limit, the exit beyond it), tail-recursive loops and tail-call
-- chains (frame replacement), calls with C = 0 feeding B = 0 calls and returns, vararg callees, errors and coroutine
-- yields in the middle of a recursion.

-- recursion: fib-like, ackermann-like, a depth that changes between rounds; one type change late
TT[#TT + 1] = function(id)
  return tpl([[
local function fib$id(n) if n < 2 then return n end return fib$id(n - 1) + fib$id(n - 2) end
local function ack$id(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack$id(m - 1, 1) end
  return ack$id(m - 1, ack$id(m, n - 1))
end
local function dep$id(n, x) if n == 0 then return x end return 1 + dep$id(n - 1, x) end
local function f$id(n, d, k)
  local s = 0
  for i = 1, n do
    s = s + fib$id(d) + ack$id(1, i % 3) + dep$id(d % 4, i)
    if i == k then s = s + dep$id(2, 0.5) end
  end
  return s
end
local function r$id()
  mix(f$id($N, 2, $K)); mix(f$id($N, 5, $K)); mix(f$id($N, 1, 1000)); mix(f$id($N, 7, $K)); mix(f$id(3, 9, 1))
  local ok, e = pcall(f$id, $N, 6, 5); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(40, 200), K = rnd(20, 90) })
end

-- tail-recursive loops, mutual tail recursion, tail-call chains with fewer / more arguments, results through the chain
TT[#TT + 1] = function(id)
  return tpl([[
local function loop$id(n, acc) if n == 0 then return acc end return loop$id(n - 1, acc + n) end
local ev$id, od$id
function ev$id(n) if n == 0 then return true end return od$id(n - 1) end
function od$id(n) if n == 0 then return false end return ev$id(n - 1) end
local function c$id(a, b, c) if c == nil then return a, b end return a + b + c, c end
local function b$id(x) return c$id(x, $A) end
local function a$id(x) local t = x + 1 return b$id(t) end
local function f$id(n, m)
  local s = 0
  for i = 1, n do
    s = s + loop$id(m + i % 3, 0)
    if ev$id(m + i % 2) then s = s + 1 end
    local p, q = a$id(i)
    s = s + p + (q or 0) + select('#', a$id(i))
    if i % $M == 0 then s = s + (c$id(i, 2, 3)) end
  end
  return s
end
local function r$id()
  mix(f$id($N, 4)); mix(f$id($N, 9)); mix(f$id($N, 0)); mix(f$id(3, 30))
  local ok, e = pcall(f$id, $N, "x"); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(40, 200), A = rnd(1, 9), M = rnd(2, 7) })
end

-- multiple results: swap / divmod, C = 0 calls feeding B = 0 calls, RETURN B = 0 nested, intrinsics fed by calls
TT[#TT + 1] = function(id)
  return tpl([[
local function swap$id(a, b) return b, a end
local function divmod$id(a, b) return a // b, a % b end
local function two$id(a) return a, a + $A end
local function three$id(a) return a, a * 2, a - 1 end
local function sum$id(a, b, c) return a + b + (c or 0) end
local function wrap$id(a) return $B, two$id(a) end
local function deep$id(a) return wrap$id(a) end
local max$id, min$id = math.max, math.min
local function f$id(n, kind)
  local s, x, y = 0, 1, 2
  for i = 1, n do
    x, y = swap$id(x, y)
    local q, r = divmod$id(i * 7 + x, 5 + y)
    s = s + q + r + sum$id(two$id(i)) + sum$id(three$id(i)) + sum$id(1, two$id(i)) + sum$id(deep$id(i))
    s = s + max$id(two$id(i)) + min$id(three$id(i))
    local p, u, v = deep$id(i)
    s = s + p + u + v
    if kind == 1 and i % $M == 0 then s = s + max$id(two$id(i + 0.5)) end
    if kind == 2 and i % $M == 0 then s = s + (select('#', two$id(i))) + (select(2, three$id(i))) end
  end
  return s, x, y
end
local function r$id()
  for kind = 0, 2 do mix(f$id($N, kind)); mix(f$id($N, kind)); mix(f$id(3, kind)) end
  local ok, e = pcall(f$id, $N, "k"); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(40, 200), A = rnd(1, 9), B = rnd(0, 9), M = rnd(2, 9) })
end

-- vararg callees and select('#', ...) next to traced code, errors and coroutine yields in the middle of a recursion
TT[#TT + 1] = function(id)
  return tpl([[
local function cnt$id(...) return select('#', ...) end
local function pass$id(...) return ... end
local function two$id(a) return a, a + 1 end
local function rec$id(n, t) if n == 0 then return t.x + 1 end return 1 + rec$id(n - 1, t) end
local function walk$id(n) if n == 0 then coroutine.yield(1) return 0 end return 1 + walk$id(n - 1) end
local function f$id(n, bad)
  local s = 0
  local ts = {}
  for i = 1, n do ts[i] = { x = i } end
  ts[bad] = $V
  for i = 1, n do
    s = s + cnt$id() + cnt$id(i, i) + cnt$id(pass$id(i, 2, 3)) + cnt$id(two$id(i))
    s = s + rec$id(i % 3, ts[i])
  end
  return s
end
local function r$id()
  mix(f$id($N, 1000)); mix(f$id($N, 1000))
  local ok, e = pcall(f$id, $N, 5); if ok then mix(e) else err(e) end
  local co = coroutine.wrap(function() local s = 0 for i = 1, $N do s = s + walk$id(i % 3) end return s end)
  local acc = 0
  for k = 1, $N + 1 do acc = acc + co() end
  mix(acc)
end
run("r$id", r$id)
]], { id = id, N = N(40, 120), V = pick({ "5", "'s'", "true", "nil" }) })
end

-- BF: table runtime shapes (array growth through the empty-hash rehash path, holes, zero/negative/float/huge int keys,
-- shrinking, pairs order): node placement, sizes and order must equal the reference's. `#` is sampled only on proper
-- sequences: on tables with holes any border is legal and the reference itself varies with the string seed
TT[#TT + 1] = function(id)
  return tpl([[
local function r$id(n)
  local function dump(t, len)
    local c, s = 0, 0
    for k, v in pairs(t) do c = c + 1; s = (s * 31 + (math.type(k) == "integer" and k or c) * 7 + (type(v) == "number" and v or 1)) % 1000003 end
    mix(c); mix(s); if len then mix(#t) end
  end
  local t = {}
  for i = 1, n do t[i] = i; if i % $P == 0 then dump(t, true) end end
  local h = {}
  for i = 1, n do if i % $H ~= 0 then h[i] = i end; end
  dump(h)
  local z = {}
  for i = $LO, $HI do z[i] = i end
  z[0] = 0; z[-1] = -1; z[math.maxinteger] = 1; z[math.mininteger] = 2; z[2^31] = 3; z[-2^31] = 4; z[1.5] = 5
  dump(z)
  local ks = {}
  for k in pairs(z) do ks[#ks + 1] = k end
  for i = 1, #ks do mix(ks[i]) end
  for i = $LO, $HI, 2 do z[i] = nil end
  dump(z)
  z[100] = 1; dump(z)
  local r = {}
  for i = n, 1, -1 do r[i] = i end
  dump(r, true)
  for i = 1, n do r[i] = nil; if i % 13 == 0 then r[n + i] = i end end
  dump(r)
  local m = {}
  for i = 1, n do m[#m + 1] = i; m[-i] = i; if i % 5 == 0 then m[i * 1000003] = i end end
  dump(m, true)
  local u = {}
  for i = 1, n do local w = {}; for j = 1, i % 9 do w[j] = j end; u[i] = #w end
  dump(u, true)
end
run("r$id", r$id, $N)
]], { id = id, N = N(30, 400), P = rnd(7, 60), H = rnd(2, 9), LO = rnd(-40, 3), HI = rnd(20, 300) })
end

local OOP0 = #TT
------------------------------------------------------------------ `__index` chain templates of the "tr" set (task P-A)
-- OOP method and field lookup through setmetatable(t, {__index = Class}) chains of depth 1-3: the instance has no own key
-- (HMISS), the class tables hold it (HREFK); then a method overridden after warm-up, class mutation inside the loop,
-- an `__index` replaced / removed / turned into a function, instance fields that shadow methods, missing keys.

-- inheritance depth D (1-3): methods defined at different levels, class fields as defaults, an override after iteration K,
-- a shadowing own field after iteration K2, a method added to the base class late
TT[#TT + 1] = function(id)
  return tpl([[
local B$id = {}; B$id.__index = B$id
B$id.base = $BV
function B$id.new(x) return setmetatable({ x = x, n = 0 }, B$id) end
function B$id:inc(d) self.x = self.x + d; self.n = self.n + 1 end
function B$id:get() return self.x end
local M$id = setmetatable({}, { __index = B$id }); M$id.__index = M$id
M$id.mid = $MV
function M$id.new(x) return setmetatable({ x = x, n = 0, w = 2 }, M$id) end
function M$id:twice(d) self:inc(d); self:inc(d) end
local L$id = setmetatable({}, { __index = M$id }); L$id.__index = L$id
function L$id.new(x) return setmetatable({ x = x, n = 0, w = 3, z = 1.5 }, L$id) end
function L$id:inc(d) self.x = self.x + d * 2; self.n = self.n + 1 end
local function mk$id(depth, x)
  if depth == 1 then return B$id.new(x) elseif depth == 2 then return M$id.new(x) else return L$id.new(x) end
end
local function f$id(o, n)
  local s = 0
  for i = 1, n do
    o:inc(i % $M)
    if o.twice then o:twice(1) end
    s = s + o:get() + o.base + (o.mid or 0) + (o.nokey or 7) + (o.w or 1)
    if i == $K then B$id.get = function(self) return self.x + 100 end end
    if i == $K2 then o.$SH = $SHV end
    if i == $K3 then B$id.base = $BV2 end
  end
  return s, o.x, o.n
end
local function r$id()
  for depth = 1, 3 do
    local o = mk$id(depth, 1)
    local a, x, y = f$id(o, $N)
    mix(a); mix(x); mix(y)
    local ok, e = pcall(f$id, o, $N)
    if ok then mix(e) else err(e) end
    local o2 = mk$id(depth, 2)
    ok, e = pcall(f$id, o2, $N)
    if ok then mix(e) else err(e) end
  end
end
run("r$id", r$id)
]], { id = id, N = N(80, 400), K = rnd(5, 60), K2 = rnd(5, 60), K3 = rnd(5, 60), M = rnd(2, 9),
    BV = pick{ "1", "2.5", "'s'", "true" }, BV2 = pick{ "9", "0.5", "nil", "{}" }, MV = pick{ "4", "false" },
    SH = pick{ "inc", "get", "base", "w", "nokey", "twice" },
    SHV = pick{ "function(self, d) self.x = self.x - 1 end", "5", "nil", "'str'" } })
end

-- the `__index` of the metatable changes: replaced by another class, by a function, removed (nil), made a non-table; the
-- metatable of the instance replaced or removed; `__index` inherited through a metatable that gets one late
TT[#TT + 1] = function(id)
  return tpl([[
local A$id = { v = 1 }; A$id.__index = A$id
function A$id:m() return self.p + self.v end
local C$id = { v = 10 }; C$id.__index = C$id
function C$id:m() return self.p * self.v end
local function mk$id(kind)
  local o = { p = 3 }
  if kind == 1 then return setmetatable(o, A$id)
  elseif kind == 2 then return setmetatable(o, setmetatable({}, { __index = A$id }))
  else return o end
end
local function f$id(o, n)
  local s = 0
  local mt = getmetatable(o)
  for i = 1, n do
    s = s + o.p + (o.v or -1)
    if o.m then s = s + o:m() end
    if i == $K then
      $MUT
    end
  end
  return s
end
local function r$id()
  for kind = 1, 3 do
    local o = mk$id(kind)
    local ok, e = pcall(f$id, o, $N)
    if ok then mix(e) else err(e) end
    ok, e = pcall(f$id, o, $N)
    if ok then mix(e) else err(e) end
    ok, e = pcall(f$id, mk$id(kind), 7)
    if ok then mix(e) else err(e) end
  end
  A$id.v = 1; C$id.v = 10; A$id.__index = A$id
end
run("r$id", r$id)
]], { id = id, N = N(60, 300), K = rnd(5, 50),
    MUT = (pick{ "A$id.__index = C$id", "A$id.__index = function(t, k) return 5 end", "A$id.__index = nil",
      "A$id.__index = 3", "setmetatable(o, C$id)", "setmetatable(o, nil)", "A$id.v = nil", "A$id.m = C$id.m",
      "o.v = 100", "rawset(o, 'm', function() return 1000 end)", "setmetatable(o, { __index = function() return 8 end })",
      "if mt then mt.__index = C$id end", "if mt then mt.__index = nil end", "A$id.__index = setmetatable({}, { __index = C$id })" }):gsub("%$id", tostring(id)) })
end

-- many instances of several classes in one loop (the metatable of the receiver varies between iterations), instances with
-- different field sets, fields inserted into the instance in the loop, `__newindex` on the class
TT[#TT + 1] = function(id)
  return tpl([[
local P$id = {}; P$id.__index = P$id
function P$id:val() return self.a + (self.b or 0) end
local Q$id = setmetatable({}, { __index = P$id }); Q$id.__index = Q$id
function Q$id:val() return self.a * 2 end
local R$id = setmetatable({}, { __index = Q$id }); R$id.__index = R$id
local N$id = setmetatable({}, { __index = P$id, __newindex = function(t, k, v) rawset(t, k, v + 1) end }); N$id.__index = N$id
local function mk$id(i)
  local c = i % 4
  local o
  if c == 0 then o = setmetatable({ a = i }, P$id)
  elseif c == 1 then o = setmetatable({ a = i, b = 2, c = 3, d = 4 }, Q$id)
  elseif c == 2 then o = setmetatable({ b = 1, a = i * 0.5 }, R$id)
  else o = setmetatable({ a = i }, N$id) end
  return o
end
local function f$id(n)
  local objs = {}
  for i = 1, 12 do objs[i] = mk$id(i) end
  local s = 0
  for i = 1, n do
    local o = objs[i % 12 + 1]
    s = s + o:val()
    if i % $M == 0 then o.$NEW = i end
    s = s + (o.$NEW or 0)
    if i == $K then Q$id.val = P$id.val end
  end
  return s
end
local function r$id()
  mix(f$id($N)); mix(f$id($N))
  Q$id.val = function(self) return self.a * 2 end
  local ok, e = pcall(f$id, $N); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(60, 300), M = rnd(3, 11), K = rnd(5, 80), NEW = pick{ "e", "b", "val", "q" } })
end

-- globals as an object namespace: `Class.method(obj)`, `obj.method` stored into a local, class tables with many fields
-- (a hash part with chains), keys that collide, deep depth 4 (the recorder's limit) and 5
TT[#TT + 1] = function(id)
  return tpl([[
local function chain$id(depth)
  local c = { f1 = 1, f2 = 2, f3 = 3, f4 = 4, f5 = 5, f6 = 6, f7 = 7, f8 = 8 }
  c.__index = c
  function c:top() return self.f1 + self.f8 end
  for d = 2, depth do
    local nx = { ["g" .. d] = d }
    nx.__index = nx
    setmetatable(nx, { __index = c })
    c = nx
  end
  return c
end
local function f$id(depth, n)
  local k = chain$id(depth)
  local o = setmetatable({ own = 1, g1 = 0.5, h = 2, i = 3 }, k)
  local s = 0
  local m = o.top
  for i = 1, n do
    s = s + m(o) + o:top() + o.f3 + (o.g2 or 0) + (o.g5 or 0) + o.own
    if i == $K then k.f3 = $V end
    if i == $K2 then o.f3 = 9 end
  end
  return s
end
local function r$id()
  for depth = 1, 6 do
    local ok, e = pcall(f$id, depth, $N)
    if ok then mix(e) else err(e) end
  end
end
run("r$id", r$id)
]], { id = id, N = N(60, 300), K = rnd(5, 50), K2 = rnd(5, 50), V = pick{ "30", "0.5", "nil", "'s'" } })
end

------------------------------------------------------------------ vararg / select / loops-in-callees templates of the "tr" set (task P-E)
-- inlined vararg callees (`VARARGPREP`, `VARARG`, `RETURN` with the vararg delta), `select('#', ...)` / `select(n, ...)`,
-- loops of callees unrolled, guards failing inside a vararg frame, errors and `pcall` around them.

TT[#TT + 1] = function(id)
  return tpl([[
local function pass$id(...) return ... end
local function cnt$id(...) return select('#', ...) end
local function nth$id(k, ...) return (select(k, ...)) end
local function fx$id(a, ...) local b, c = ... return a, b, c end
local function sum$id(...) local s = 0 for i = 1, select('#', ...) do s = s + (select(i, ...)) end return s end
local function f$id(n, bad)
  local s = 0
  for i = 1, n do
    local a, b, c = pass$id(i, $A, i + 1)
    s = s + a + b + c + cnt$id(a, b) + nth$id($K, i, 2, 3) + sum$id(i, $A, $B)
    local x, y, z = fx$id(i, i)
    s = s + x + (y or 0) + (z or 0)
    if i == bad then s = s + sum$id(i, $V) end
  end
  return s
end
local function r$id()
  mix(f$id($N, 1000)); mix(f$id($N, 1000)); mix(f$id(3, 1000))
  local ok, e = pcall(f$id, $N, $N - 3); if ok then mix(e) else err(e) end
end
run("r$id", r$id)
]], { id = id, N = N(40, 200), A = rnd(1, 9), B = rnd(0, 9), K = rnd(1, 3), V = pick({ "'5'", "'s'", "nil", "2.5", "true" }) })
end

-- a late type change and a different argument count inside vararg frames; multiple results through `...`; vararg callees that
-- call vararg callees; tail calls and recursion through `...` (aborts); a loop of a callee longer than the unroll limit
TT[#TT + 1] = function(id)
  return tpl([[
local function pass$id(...) return ... end
local function twice$id(...) return pass$id(...), pass$id(...) end
local function mid$id(a, ...) return a + 1, ... end
local function over$id(f, ...) return f(...) end
local function cnt$id(n, ...) if n == 0 then return select('#', ...) end return cnt$id(n - 1, n, ...) end
local function long$id(...) local s = 0 for i = 1, select('#', ...) do s = s + (select(i, ...)) end return s end
local function f$id(n, late)
  local s = 0
  for i = 1, n do
    local a = i
    if i > late then a = i + 0.5 end
    local r, x, y = mid$id(a, i, 3)
    s = s + r + x + y + select('#', twice$id(i, 2)) + (select(2, over$id(twice$id, 5, 6)))
    s = s + cnt$id(i % 3, i)
    if i % $M == 0 then s = s + long$id(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28) end
  end
  return s
end
local function r$id()
  mix(f$id($N, 1000)); mix(f$id($N, $N - 10)); mix(f$id(4, 1))
  local co = coroutine.wrap(function(...) local s = 0 for i = 1, $N do s = s + select('#', ...) + (coroutine.yield(i)) end return s end)
  local acc = co(1, 2, 3)
  for k = 1, $N do acc = acc + (co(k) or 0) end
  mix(acc)
end
run("r$id", r$id)
]], { id = id, N = N(40, 160), M = rnd(5, 13) })
end


------------------------------------------------------------------ function-entry roots and table.pack templates of the "tr" set (task P-E2)
-- recursion shapes called from straight-line code, callbacks and loops: fib, mutual recursion, a recursion whose depth varies
-- between calls, tail calls, argument types that change under a hot function, `table.pack` / `{...}` of fixed counts.

TT[#TT + 1] = function(id)
  return tpl([[
local function fib$id(n) if n < 2 then return n end return fib$id(n - 1) + fib$id(n - 2) end
local ev$id, od$id
function ev$id(n) if n == 0 then return true end return od$id(n - 1) end
function od$id(n) if n == 0 then return false end return ev$id(n - 1) end
local function dp$id(n) if n == 0 then return 0 end return 1 + dp$id(n - 1) end
local function tl$id(n, acc) if n == 0 then return acc end return tl$id(n - 1, acc + n) end
local function f$id(n)
  local s = 0
  for i = 1, n do
    s = s + fib$id(i % $K) + dp$id(i % $D) + tl$id(i % 7, $A) + ((ev$id(i % 9)) and 1 or 2)
  end
  return s
end
local function r$id()
  mix(f$id($N)); mix(f$id(7)); mix(fib$id($K + 3)); mix(dp$id($D * 2))
  local t = {}
  for i = 1, 40 do t[i] = (i * 7) % 11 end
  table.sort(t, function(a, b) return fib$id(a % 5) + a < fib$id(b % 5) + b end)
  mix(table.concat(t, ","))
  mix(select('#', ev$id(3), od$id(4)))
end
run("r$id", r$id)
]], { id = id, N = N(30, 200), K = rnd(3, 9), D = rnd(2, 12), A = rnd(0, 5) })
end

-- errors inside a recursion (at varying depths, with values of several types), coroutines that yield from inside a recursion,
-- hooks installed while the recursion is hot
TT[#TT + 1] = function(id)
  local h = hookspec()
  return tpl([[
local function boom$id(n, k) if n == k then error(n % 2 == 0 and "e" .. n or {code = n}) end return boom$id(n + 1, k) + 1 end
local function walk$id(n) if n > 0 then walk$id(n - 1) coroutine.yield(n) walk$id(n - 1) end end
local function yc$id(n) if n == 0 then return 0 end local x = coroutine.yield(n) return (x or 0) + yc$id(n - 1) end
local function fib$id(n) if n < 2 then return n end return fib$id(n - 1) + fib$id(n - 2) end
local hk$id = 0
local function f$id(n)
  local s = 0
  for i = 1, n do
    local ok, e = pcall(boom$id, 0, i % 6)
    s = s + (ok and 1 or (type(e) == "table" and e.code or #e)) + fib$id(i % 8)
  end
  return s
end
local function r$id()
  mix(f$id($N))
  local c = 0
  for v in coroutine.wrap(function() walk$id($W) end) do c = c + v end
  mix(c)
  local co = coroutine.wrap(function() return yc$id(5) end)
  local r = co()
  for i = 1, 5 do r = co(i) end
  mix(r)
  debug.sethook(function() hk$id = hk$id + 1 end, $MASK, $CNT)
  mix(fib$id(9)); mix(f$id(8))
  rehook()
  mix(hk$id > 0 and 1 or 0)
  mix(f$id(8))
end
run("r$id", r$id)
]], { id = id, N = N(30, 150), W = rnd(2, 6), MASK = h[1], CNT = h[2] })
end

-- argument types that change under a hot function; `table.pack` and `{...}` of fixed counts with nil and collectable values
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(a, b) if a < b then return a + b end return a - b end
local function g$id(x) return f$id(x, x + 1) + f$id(x + 1, x) end
local function pk$id(...) local t = table.pack(...) return t.n, t[1], t[t.n], #t end
local function pe$id(...) local t = {...} return #t, t[1] end
local function pm$id(a, b) local t = table.pack(a, {b}, "s" .. b, nil) return t.n + #t[2] + #t[3] + (t[1] or 0) end
local function r$id()
  local s = 0
  for i = 1, $N do s = s + g$id(i) end
  for i = 1, $N do s = s + g$id(i + 0.25) end
  for i = 1, $N do
    local n, a, b, l = pk$id(i, $V, i * 2)
    s = s + n + a + (b or 0) + l
    local m, x = pe$id(i, i + 1)
    s = s + m + x + pm$id(i, i + 2)
  end
  mix(s)
  local ok, e = pcall(g$id, "s"); mix(ok and 1 or 2)
  mix(pk$id()); mix(select('#', pk$id(nil, nil)))
end
run("r$id", r$id)
]], { id = id, N = N(30, 200), V = pick({ "nil", "2.5", "'x'", "true", "7" }) })
end


-- P-B: library-call intrinsics: string.sub / byte / char / len, the method forms (`s:sub`), type / math.type / tostring of numbers,
-- with edge indices, a long string, a late replacement of a library function, a string metatable change, a number metatable
-- (`__tostring`), allocation pressure from the strings the loop makes
TT[#TT + 1] = function(id)
  return tpl([[
local S$id = { "hello world", "", "x", ("abcdefghij"):rep(7), "\0\1\2", "a longer string than forty bytes: 0123456789 abc" }
local function f$id(n, late)
  local s, acc = 0, ""
  for i = 1, n do
    local str = S$id[i % #S$id + 1]
    local a = str:sub(i % 5 - 1, i % 7 - 2)
    local b = string.sub(str, $A, $B)
    local c = str:byte(i % 9 - 3)
    local d = string.char(i % 256)
    local e = tostring(i) .. tostring(i / 8) .. type(i) .. type(str) .. type(nil) .. tostring(math.type(i)) .. tostring(math.type(str))
    s = s + #a + #b + (c or -1) + #d + #e + #str:sub(2)
    if i % $M == 0 then acc = acc .. a:sub(1, 3) .. string.char(65 + i % 26) end
    if i == late then $MUT end
  end
  return s, #acc
end
local function r$id()
  local keep = { string.sub, string.byte, string.char, tostring, type }
  for _, args in ipairs{ { $N, 1000 }, { $N, $K }, { 7, 1000 } } do
    local ok, a, b = pcall(f$id, args[1], args[2])
    if ok then mix(a); mix(b) else err(a) end
  end
  string.sub, string.byte, string.char, tostring, type = keep[1], keep[2], keep[3], keep[4], keep[5]
  debug.setmetatable(0, nil); getmetatable("").__index = string
end
run("r$id", r$id)
]], { id = id, N = N(40, 200), K = rnd(5, 35), M = rnd(3, 11), A = pick{ "1", "-1", "0", "3", "-3" }, B = pick{ "-1", "2", "100", "-100", "0" },
    MUT = pick{ "string.sub = function(s, i, j) return 'S' end", "string.byte = function() return 7 end", "string.char = function(c) return 'c' end",
      "tostring = function() return 'T' end", "type = function() return 'ty' end", "debug.setmetatable(0, { __tostring = function() return 'N' end })",
      "getmetatable('').__index = function(s, k) return string[k] end", "getmetatable('').__index = { sub = function() return 'M' end, byte = string.byte }",
      "string.sub = nil", "getmetatable('').__index = nil", "debug.setmetatable(0, { __index = function() return 1 end })" } })
end

-- P-B: table.insert / table.remove (append and pop forms), rawlen, math.fmod / math.tointeger / tonumber of numbers, with a
-- `__len` / `__newindex` metatable added late, a sparse table, an empty table and a shrinking one
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(t, n, late)
  local s = 0
  for i = 1, n do
    table.insert(t, i)
    if i % $M == 0 then s = s + (table.remove(t) or 0) end
    s = s + rawlen(t) + #t
    local x = i + 0.5
    s = s + math.fmod(x, 3.25) + (math.tointeger(i + 0.0) or 0) + tonumber(i) + tonumber(x) + (math.tointeger(x) or -1)
    if i == late then $MUT end
  end
  return s, #t, t[1], t[#t]
end
local function r$id()
  local keep = { table.insert, table.remove, rawlen, math.fmod, tonumber }
  local ok, a, b, c, d = pcall(f$id, {}, $N, 1000); if ok then mix(a); mix(b); mix(c); mix(d) else err(a) end
  ok, a, b, c, d = pcall(f$id, { 1, 2, 3 }, $N, $K); if ok then mix(a); mix(b); mix(c); mix(d) else err(a) end
  ok, a, b, c, d = pcall(f$id, setmetatable({}, { __len = function() return 2 end }), 7, 1000); if ok then mix(a); mix(b) else err(a) end
  ok, a, b, c, d = pcall(f$id, { n = 1, [100] = 5 }, $N, 1000); if ok then mix(a); mix(b); mix(c) else err(a) end
  local e = {}
  for k = 1, 5 do mix(table.remove(e)) end
  ok, a = pcall(table.insert, e, 1, 2, 3); if not ok then err(a) end
  ok, a = pcall(table.insert, nil, 1); if not ok then err(a) end
  table.insert, table.remove, rawlen, math.fmod, tonumber = keep[1], keep[2], keep[3], keep[4], keep[5]
  ok, a = pcall(math.fmod, 1, 0); if not ok then err(a) end
end
run("r$id", r$id)
]], { id = id, N = N(40, 200), K = rnd(5, 35), M = rnd(3, 9),
    MUT = pick{ "setmetatable(t, { __newindex = function(t, k, v) rawset(t, k, v * 2) end })", "setmetatable(t, { __len = function() return 0 end })",
      "t[#t + 5] = 1", "table.insert = function() end", "table.remove = function() return 'r' end", "rawlen = function() return 1 end",
      "math.fmod = function() return 2.5 end", "tonumber = function(v) return 3 end", "setmetatable(t, nil)", "for k = 1, #t do t[k] = nil end" } })
end

-- P-F: loops whose step is computed at run time (not a literal): the sieve shape (step = outer index), steps of both signs
-- for the same loop, skipped loops, differences that do not fit a signed integer, step 0 and mininteger (errors / Nyi)
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(n, st, a, b)
  local s = 0
  for i = 1, n do
    for j = a, b, st do s = s + j end
    for j = i * 2, b + i, st do s = s - j end
  end
  return s
end
local function sv$id(m)
  local flags, c = {}, 0
  for i = 2, m do flags[i] = true end
  for i = 2, m do
    if flags[i] then
      for j = i * i, m, i do flags[j] = false end
    end
  end
  for i = 2, m do if flags[i] then c = c + 1 end end
  return c
end
local function r$id()
  for k = 1, $N do
    local st = 1 + (k * $K) % 5
    mix(f$id(8, st, 1, $M)); mix(f$id(8, -st, $M, 1)); mix(f$id(6, st, $M, 1)); mix(f$id(6, -st, 1, $M))
  end
  mix(f$id(5, math.maxinteger // 2, -5, math.maxinteger)); mix(f$id(5, math.mininteger // 2, 5, math.mininteger))
  mix(sv$id($M * 20))
  local ok, v = pcall(f$id, 3, 0, 1, 5); if ok then mix(v) else err(v) end
  ok, v = pcall(f$id, 3, math.mininteger, 5, 1); if ok then mix(v) else err(v) end
end
run("r$id", r$id)
run("r$id", r$id)
]], { id = id, N = N(30, 150), K = rnd(1, 7), M = rnd(20, 60) })
end

-- P-N: `%` and `//` by constant powers of two (AND / arithmetic shift in the trace) with dividends of both signs,
-- mininteger / maxinteger neighbours, a float dividend, and the 2^31 / 2^32 boundary of the immediate mask
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(n, off)
  local s, t = 0, 0
  for i = 1, n do
    local a = (i * $K) % 301 - 150 + off
    s = s + a % $P + a // $P + (a - $M) % 2 + (a - $M) // 2 + (-a) % 8 + (-a) // 8
    local b = math.mininteger + i + off
    local c = math.maxinteger - i - off
    t = t + b % 16 + b // 16 + c % 16 + c // 16 + b % 2147483648 + c // 2147483648 + b % 4294967296 + c // 4294967296
    s = s + (a % 4 > 1 and 1 or 0) + (a // 2 > 3 and 2 or 0)
    if i % $L == 0 then s = s + ((a + 0.5) % 4) end
  end
  return s, t
end
local function r$id()
  for k = 1, $N do mix(f$id(40, k)); mix(f$id(40, -k)) end
  local ok, a, b = pcall(f$id, 80, 0); if ok then mix(a); mix(b) else err(a) end
end
run("r$id", r$id)
run("r$id", r$id)
]], { id = id, N = N(20, 100), K = rnd(3, 997), M = rnd(1, 9), L = rnd(10, 35), P = pick{ 2, 4, 8, 64, 1024 } })
end

-- P-M3: native self-recursion (fib shape): hot recursion with different arguments and argument types per call, a recursion
-- that overflows the stack inside pcall, mutual recursion, vararg recursion, recursion that allocates (GC, stack growth),
-- an error with a traceback from depth, a coroutine yielding from depth, a hook set in the middle of a recursion
TT[#TT + 1] = function(id)
  return tpl([[
local function fb$id(n) if n < 2 then return n end return fb$id(n - 1) + fb$id(n - 2) end
local function fa$id(n) if n < 3 then return 1 end return fa$id(n - 1) + fa$id(n - 2) + fa$id(n - 3) end
local ev$id, od$id
function ev$id(n) if n == 0 then return true end return od$id(n - 1) end
function od$id(n) if n == 0 then return false end return ev$id(n - 1) end
local function va$id(n, ...) if n == 0 then return select('#', ...) end return va$id(n - 1, n, ...) end
local function ds$id(n) if n == 0 then return {} end local t = ds$id(n - 1); t[#t + 1] = n; return t end
local function dp$id(n) return 1 + dp$id(n + 1) end
local function er$id(n) if n == 0 then error("deep$id") end return er$id(n - 1) + 1 end
local function yl$id(n) if n == 0 then coroutine.yield(7) return 0 end return yl$id(n - 1) + 1 end
local function hk$id(n)
  if n == $HK then debug.sethook(function() HC = (HC or 0) + 1 end, "", 50) end
  if n < 2 then return n end
  return hk$id(n - 1) + hk$id(n - 2)
end
local function ty$id(n) if n < 2 then return n end return ty$id(n - 1) + ty$id(n - 2) end
local function r$id()
  for k = 1, $N do
    mix(fb$id($M + k % 4)); mix(fa$id($M + 2)); mix(ev$id(k % 7 + 1)); mix(od$id(k % 5 + 1))
  end
  mix(fb$id($M + 5))
  mix(va$id($M)); mix(#ds$id($M * 3))
  local ok, v = pcall(dp$id, 1); if ok then mix(v) else err(v) end
  ok, v = xpcall(er$id, debug.traceback, $M); if ok then mix(v) else err(v) end
  local co = coroutine.wrap(function() return yl$id($M) end)
  mix(co()); mix(co())
  for k = 1, 30 do mix(fb$id(4 + k % 3)) end
  mix(ty$id(6.0)); mix(ty$id(7)); mix(ty$id(5.5))
  ok, v = pcall(ty$id, "x"); if not ok then err(v) end
  ok, v = pcall(ty$id, nil); if not ok then err(v) end
  mix(ty$id(8))
  mix(hk$id($M + 3)); debug.sethook()
  mix(fb$id($M)); collectgarbage()
end
run("r$id", r$id)
]], { id = id, N = N(20, 120), M = rnd(5, 11), HK = rnd(2, 5) })
end

-- P-X: a float dividend appearing mid-loop (`a` retyped in place) followed by `a % 4 > 1 and 1 or 0` (regression shape of
-- the side-trace miscompile of docs/JIT_TRACE_REF.md "P-X decisions and results")
TT[#TT + 1] = function(id)
  return tpl([[
local function f$id(n, off)
  local s, t = 0, 0
  for i = 1, n do
    local a = (i * $K) % 301 - 150 + off
    s = s + a % $P + a // $P + (a - $M) % 2 + (a - $M) // 2 + (-a) % 8 + (-a) // 8
    if i == $L then a = a + 0.5 end
    s = s + (a % 4 > 1 and 1 or 0) + (a // 2 > 3 and 2 or 0)
  end
  return s, t
end
local function r$id()
  for k = 1, $N do mix(f$id(40, k)); mix(f$id(40, -k)) end
  local ok, a, b = pcall(f$id, 80, 0); if ok then mix(a); mix(b) else err(a) end
end
run("r$id", r$id)
run("r$id", r$id)
]], { id = id, N = N(20, 100), K = rnd(3, 997), M = rnd(1, 9), L = rnd(10, 35), P = pick{ 2, 4, 8, 64, 1024, 1000 } })
end

-- P-V: string-method calls (`s:sub`, `s:byte`, a user method) in a hot loop while the string metatable, its `__index`, the table
-- behind it or one method is changed mid-loop (the trace resolves `s:method` through a constant metatable + guarded loads)
TT[#TT + 1] = function(id)
  local muts = {
    "string.sub = function(s, i, j) return \"q\" end",
    "string.sub = function(s, i, j) return string.upper(s) end",
    "string.byte = nil",
    "string.zz$id = function(s) return 9 end",
    "local m = getmetatable(\"\"); m.__index = setmetatable({}, { __index = string })",
    "local m = getmetatable(\"\"); m.__index = function(s, k) return string[k] end",
    "local m = getmetatable(\"\"); m.__index = { sub = string.sub, byte = string.byte, zz$id = function() return 3 end }",
    "debug.setmetatable(\"\", { __index = { sub = string.sub, byte = string.byte, zz$id = function() return 4 end } })",
    "debug.setmetatable(\"\", nil)",
    "for i = 1, 40 do string[\"x$id_\" .. i] = i end",
  }
  return tpl([[
local sub$id, byte$id, mt$id = string.sub, string.byte, getmetatable("")
local idx$id = mt$id.__index
string.zz$id = function(s) return #s + 2 end
local function f$id(s, n, k)
  local acc = 0
  for i = 1, n do
    if i == k then $MUT end
    local c = s:sub(i % $P + 1, i % $P + $Q)
    acc = acc + #c + (s:byte(i % $R + 1) or 0) + s:zz$id()
  end
  return acc
end
local function r$id()
  for _, k in ipairs{ $K1, $K2, 1000000 } do
    local ok, a = pcall(f$id, ("abcdefghij"):rep($Z), $N, k)
    if ok then mix(a) else err(a) end
    string.sub, string.byte = sub$id, byte$id
    string.zz$id = function(s) return #s + 2 end
    if getmetatable("") ~= mt$id then debug.setmetatable("", mt$id) end
    mt$id.__index = idx$id
  end
end
run("r$id", r$id)
run("r$id", r$id)
]], { id = id, MUT = (pick(muts):gsub("%$id", tostring(id))), N = N(60, 400), K1 = rnd(1, 30), K2 = rnd(31, 300), P = rnd(2, 6), Q = rnd(0, 4), R = rnd(3, 11), Z = rnd(1, 3) })
end

-- P-O: `{...}` / `{n = select('#', ...), ...}` with 0 / 1 / many / nil-containing varargs and `pcall` / `xpcall` of erroring and
-- non-erroring closures in hot loops: error values of several types, an error inside a nested pcall, an xpcall handler with a
-- traceback, pcall of a non-function / callable table, a to-be-closed variable in the callee, a stack overflow inside pcall,
-- a hook set in the middle of the loop
TT[#TT + 1] = function(id)
  return tpl([[
local function pk$id(...) local t = {...}; return #t, t[1], t[2], t[$J] end
local function pn$id(...) local t = {n = select('#', ...), ...}; return t.n, #t, t[1] end
local function pt$id(...) local t = {..., $K}; return #t, t[1], t[2] end
local function pl$id(a, ...) local t = {a, ...}; local s = 0; for i = 1, #t do s = s + (tonumber(t[i]) or 0) end return s end
local function ok$id(i) return i + 1, i end
local function bad$id(i)
  local m = i % $M
  if m == 0 then error("bad$id " .. i)
  elseif m == 1 then error({code = i})
  elseif m == 2 then error(i)
  elseif m == 3 then error(nil)
  elseif m == 4 then error(setmetatable({}, {__tostring = function() return "tos" end}))
  elseif m == 5 then local x = nil; return x.y
  elseif m == 6 then return i + {}
  end
  return i
end
local cl$id = 0
local function tb$id(i)
  local x <close> = setmetatable({}, {__close = function(_, e) cl$id = cl$id + 1; mix(e) end})
  if i % $M == 0 then error("tbc$id") end
  return i
end
local function deep$id(n) return 1 + deep$id(n + 1) end
local function inner$id(i)
  local ok, e = pcall(bad$id, i)
  if i % $M == 1 then error("outer$id " .. tostring(ok)) end
  return ok, e
end
local function r$id()
  for k = 1, $N do
    mix(select('#', pk$id())); mix(select('#', pk$id(k))); mix(select('#', pk$id(k, nil, k)))
    mix(select('#', pk$id(nil, nil))); mix(select('#', pk$id(k, k, k, k, k, k)))
    local a, b, c = pn$id(k, nil, k); mix(a); mix(b); mix(c)
    a, b, c = pn$id(); mix(a); mix(b); mix(c)
    a, b, c = pt$id(k, k); mix(a); mix(b); mix(c)
    mix(pl$id(k, 1, "2", 3.5))
    local ok, v, w = pcall(ok$id, k); mix(ok); mix(v); mix(w)
    ok, v = pcall(bad$id, k); mix(ok); if ok then mix(v) else err(v) end
    ok, v = xpcall(bad$id, function(m) return "H" .. (type(m) == "table" and "T" or tostring(m)) end, k); mix(ok); if ok then mix(v) else err(v) end
    ok, v = pcall(tb$id, k); mix(ok); if ok then mix(v) else err(v) end
    ok, v, w = pcall(inner$id, k); mix(ok); if ok then mix(v); mix(w) else err(v) end
  end
  mix(cl$id)
  local ok, v = xpcall(bad$id, debug.traceback, $M); mix(ok); err(v)
  ok, v = xpcall(inner$id, debug.traceback, $M + 1); mix(ok); err(v)
  for _, f in ipairs{ 42, "s", {}, setmetatable({}, {__call = function(_, x) return x end}) } do
    ok, v = pcall(f, 5); mix(ok); if ok then mix(v) else err(v) end
  end
  ok, v = pcall(nil); mix(ok); err(v)
  ok, v = pcall(); mix(ok); err(v)
  ok, v = pcall(deep$id, 1); mix(ok); err(v)
  ok, v = xpcall(deep$id, function(m) return "SO" end, 1); mix(ok); err(v)
  for i = 1, $N do
    if i == $HK then debug.sethook(function() HC = (HC or 0) + 1 end, "", 7) end
    ok, v = pcall(bad$id, i); mix(ok)
    mix(#{select(1, pk$id(i, i))})
  end
  debug.sethook()
  collectgarbage()
end
run("r$id", r$id)
run("r$id", r$id)
]], { id = id, N = N(20, 120), M = rnd(3, 9), J = rnd(1, 4), K = rnd(1, 99), HK = rnd(5, 15) })
end

-- N2: positional table.insert / table.remove / table.move (bulk element moves), with metatables, holes and resizing
TT[#TT + 1] = function(id)
  return tpl([[
local function mkt$id(n, kind)
  local t = {}
  for i = 1, n do t[i] = (i * $A) % 97 end
  if kind == 1 then return setmetatable(t, {__name = "n$id"})
  elseif kind == 2 then return setmetatable(t, {__index = function(_, k) return -k end})
  elseif kind == 3 then return setmetatable(t, {__newindex = function(tt, k, v) rawset(tt, k, v) end})
  elseif kind == 4 then t[$K % n + 1] = nil; return t
  elseif kind == 5 then local h = {}; for i = n, 1, -1 do h[i] = t[i] end; return h
  elseif kind == 6 then return setmetatable(t, {__len = function(tt) return rawlen(tt) end}) end
  return t
end
local function chk$id(t, n)
  local s = #t
  for i = 1, n do local v = rawget(t, i); s = s * 31 + (v == nil and 0 or v) end
  return s
end
local function r$id()
  for kind = 0, 6 do
    local t = mkt$id($N, kind)
    for i = 1, $N2 do
      local n = #t
      local m = i % 4
      if m == 0 then table.insert(t, 1, i)
      elseif m == 1 then table.insert(t, n // 2 + 1, i)
      elseif m == 2 then mix(table.remove(t, 1))
      else mix(table.remove(t, n // 3 + 1)) end
      if i % 7 == 0 then table.insert(t, #t + 1, i) end
      if i % 11 == 0 and #t > 3 then table.move(t, 2, #t, 1) elseif i % 13 == 0 then table.move(t, 1, #t, 2) end
    end
    mix(chk$id(t, #t + 2))
    local ok, e = pcall(table.insert, t, #t + 2, 1); if ok then mix(1) else err(e) end
    ok, e = pcall(table.insert, t, 0, 1); if ok then mix(1) else err(e) end
    ok, e = pcall(table.remove, t, #t + 2); if ok then mix(e) else err(e) end
    mix(table.remove({}, 0)); mix(table.remove({}))
  end
  local g = {}
  for i = 1, $N do table.insert(g, 1, i) end
  mix(chk$id(g, $N))
  for i = 1, $N do mix(table.remove(g, 1)) end
  mix(#g)
end
run("r$id", r$id)
]], { id = id, A = rnd(3, 50), K = rnd(1, 40), N = N(40, 200), N2 = N(30, 120) })
end

-- N5: strings of both variants (short <= 40 bytes, long > 40) and callees of mixed variants (Lua closure, light C function) read from
-- tables in loops: the string load guards "some string" where nothing observes the variant, and keeps the exact tag where something does
TT[#TT + 1] = function(id)
  return tpl([[
local S$id, O$id = {}, {}
for k = 1, $NS do S$id[k] = string.rep("x", $L0 + k) end
for k = 1, 5 do O$id[k] = {name = string.rep("y", $L0 + k * 2), n = k} end
local F$id = {function(x) return x + $K end, math.abs, function(x) return x * 2 end, math.floor, tostring}
local V$id = {1, 2.5, 3, 4.25, -5, 6}
local function unused$id(n) local acc = 0 for i = 1, n do local s = S$id[i % $NS + 1] acc = acc + i % 3 end return acc end
local function field$id(n) local acc = 0 for i = 1, n do local s = O$id[i % 5 + 1].name acc = acc + i % 5 end return acc end
local function carry$id(n) local last = S$id[1] for i = 1, n do local s = S$id[i % $NS + 1] if i % 5 == 0 then last = s end end return last end
local function early$id(n, k) for i = 1, n do local s = S$id[i % $NS + 1] if i == k then return s end end return false end
local function len$id(n) local acc = 0 for i = 1, n do acc = acc + #S$id[i % $NS + 1] end return acc end
local function eq$id(n) local t, c = S$id[$NS // 2], 0 for i = 1, n do local s = S$id[i % $NS + 1] if s == t then c = c + 1 end end return c end
local function key$id(n) local m, c = {}, 0 for i = 1, n do local s = S$id[i % $NS + 1] m[s] = (m[s] or 0) + 1 end for _, v in pairs(m) do c = c + v end return c end
local function cat$id(n) local acc = 0 for i = 1, n do acc = acc + #(S$id[i % $NS + 1] .. "!") end return acc end
local function call$id(n) local acc = 0 for i = 1, n do local f = F$id[i % 4 + 1] acc = acc + f(i) end return acc end
local function call2$id(n) local acc = 0 for i = 1, n do local r = F$id[i % 5 + 1](i) acc = acc + #tostring(r) end return acc end
local function num$id(n) local acc = 0 for i = 1, n do acc = acc + V$id[i % 6 + 1] end return acc end
local function r$id()
  for round = 1, 4 do
    local n = $N + round * 17
    mix(unused$id(n)); mix(field$id(n)); mix(#carry$id(n)); mix(carry$id(n) == S$id[(n - n % 5) % $NS + 1])
    local e = early$id(n, 3 + round); mix(#e); mix(e == S$id[(3 + round) % $NS + 1])
    mix(len$id(n)); mix(eq$id(n)); mix(key$id(n)); mix(cat$id(n)); mix(call$id(n)); mix(call2$id(n)); mix(num$id(n))
  end
  local last = carry$id($N)
  mix(#last); mix(last:sub(1, 3)); mix(type(last)); mix(#(last .. last))
end
run("r$id", r$id)
]], { id = id, NS = rnd(4, 9), L0 = rnd(30, 38), K = rnd(1, 9), N = N(60, 300) })
end

-- N1: closures called from a loop that capture locals of the loop's own frame: read-only constants, counters written by the callee
-- and by the loop itself, a value whose type the callee changes, a table replaced by the callee, a pcall with an error that leaves
-- the callee's write behind, an early return through an exit that closes the upvalue, and two closures of one proto (one captures
-- this frame's local, the other an earlier activation's closed variable) called from one call site
TT[#TT + 1] = function(id)
  return tpl([[
local saved$id
local function r$id()
  local c, K, v, t = $C0, $K, 1, {}
  local function inc() c = c + 1 return c end
  local function addk(x) return x + K end
  local function swap(i) if i % $M == 0 then c = c + 3 elseif i % 7 == 0 then c = -c end end
  local function put(i) t[#t + 1] = i end
  local function reset() t = {n = 0} end
  local function setv(i) if i % $M2 == 0 then v = "s" .. i elseif i % 5 == 0 then v = i + 0.5 elseif i % 11 == 0 then v = nil else v = i end end
  local function boom() c = c + 1 if c % $M3 == 0 then error("e" .. c) end return c end
  local acc = 0
  for i = 1, $N do
    acc = acc + inc() + addk(i)
    swap(i)
    if i % 9 == 0 then c = c + 1 end
    put(i)
    if i % $M4 == 0 then reset() end
    setv(i)
    local x = v
    if type(x) == "number" then acc = acc + x elseif type(x) == "string" then acc = acc + #x elseif x then acc = acc + 1000 else acc = acc - 1 end
    local ok, e = pcall(boom)
    if ok then acc = acc + e else acc = acc + #e end
    if i % 13 == 0 then mix(c); mix(#t); mix(tostring(v)) end
  end
  mix(acc); mix(c); mix(#t); mix(tostring(v)); mix(tostring(t.n))
end
local function body$id(n, ext)
  local c = $C0
  local function inner() c = c + 1 return c end
  local F = {inner, ext or inner}
  local s = 0
  for i = 1, n do s = s + F[i % 2 + 1]() if i % $M == 0 then c = c + 2 end end
  saved$id = inner
  return s, c
end
local function early$id(n)
  local x = 0
  local function g() x = x + $K return x end
  for i = 1, n do
    g()
    if i == n - $M then return g, x end
  end
end
local function all$id()
  r$id()
  mix(body$id($N, nil))
  for k = 1, 3 do mix(body$id($N, saved$id)) end
  for k = 1, 3 do local g, x = early$id($N + k) mix(x); mix(g()); mix(g()) end
end
run("r$id", all$id)
]], { id = id, C0 = rnd(0, 20), K = rnd(1, 9), M = rnd(3, 8), M2 = rnd(15, 40), M3 = rnd(7, 19), M4 = rnd(20, 60), N = N(80, 240) })
end

-- N6: word-frequency loops over computed short-string keys (`t[k] = (t[k] or 0) + 1`): a node vector with chains, keys that are removed
-- (nil-valued present keys) and come back, a rehash (growth and shrink) between the rounds, long strings and a number among the keys,
-- two tables swapped under the loop, and a metatable with __index / __newindex that appears in the middle of a run
TT[#TT + 1] = function(id)
  return tpl([==[
local K$id = {}
for i = 1, $NK do K$id[i] = "w" .. i .. "$P" end
for i = 1, $NL do K$id[#K$id + 1] = "L" .. ("y"):rep(41 + i) end
K$id[#K$id + 1] = $NUMKEY
local N$id = #K$id
local function freq$id(t, n, rounds)
  local s = 0
  for r = 1, rounds do
    for i = 1, n do
      local k = K$id[i % N$id + 1]
      t[k] = (t[k] or 0) + 1
      if i % $M == 0 then t[k] = nil elseif i % $M2 == 0 then s = s + t[k] end
    end
  end
  return s
end
local function copy$id(n) local B = {} for i = 1, N$id do B[i] = K$id[i] end return B end
local function two$id(t, n)
  local B = copy$id()
  local s = 0
  for i = 1, n do
    local a, b = K$id[i % N$id + 1], B[(i * 3) % N$id + 1]
    t[a] = (t[a] or 0) + i
    s = s + (t[b] or -1)
    t[b] = (t[b] or 0) + 1
    s = s + t[a]
  end
  return s
end
local function r$id()
  local t = {}
  for i = 1, $INIT do t[K$id[i]] = 0 end
  for round = 1, 5 do
    mix(freq$id(t, $N + round * 13, 2))
    mix(two$id(t, $N + round))
    if round == 2 then for i = 1, $INIT do t[K$id[i]] = nil end t.zz1 = 1 t.zz2 = 2 end
    if round == 3 then for i = 1, N$id do t[K$id[i]] = i end end
    if round == 4 then setmetatable(t, { __index = function(_, k) return #k end, __newindex = function(tt, k, v) rawset(tt, k, v) mix(k) end }) end
  end
  local keys = {}
  for k in pairs(t) do keys[#keys + 1] = tostring(k) end
  table.sort(keys)
  for i = 1, #keys do mix(keys[i]) mix(t[keys[i]]) end
  local u, v = {}, {}
  for i = 1, N$id do u[K$id[i]] = 1 v[K$id[i]] = 2 end
  local cur, s = u, 0
  for i = 1, $N do
    if i % $M3 == 0 then cur = (cur == u) and v or u end
    local k = K$id[i % N$id + 1]
    cur[k] = cur[k] + 1
    s = s + cur[k]
  end
  mix(s)
end
run("r$id", r$id)
]==], { id = id, NK = rnd(6, 40), NL = rnd(0, 3), P = rnd(1, 99), NUMKEY = rnd(1, 50), M = rnd(5, 29), M2 = rnd(3, 11), M3 = rnd(11, 60), INIT = rnd(1, 6), N = N(80, 260) })
end

-- N7: nested generic `for` over `ipairs(t)` / `pairs(t)` created in a hot loop: inner tables of 0..$MAXN elements (the ends of the traversals
-- are the side traces behind the loops), holes, array + hash, a metatable with `__index` / `__pairs` appearing and disappearing in the
-- middle of a run, `ipairs` / `pairs` shadowed for a while, a non-table argument that raises, results of the call used in odd places
TT[#TT + 1] = function(id)
  return tpl([==[
local G$id = {}
for i = 1, $NT do
  local t = {}
  for j = 1, (i * $A) % ($MAXN + 1) do t[j] = j * $B + i end
  if i % 3 == 0 then t.x$id = i t.y$id = i + 1 end
  if i % 4 == 0 and #t > 2 then t[2] = nil end
  if i % 5 == 0 then t[#t + 3] = 9 end
  G$id[i] = t
end
local function nest$id(rounds)
  local s = 0
  for r = 1, rounds do
    for ti = 1, #G$id do
      local t = G$id[ti]
      for i, v in ipairs(t) do s = s + i * v end
      for k, v in pairs(t) do s = s + v + (type(k) == "number" and k or 7) end
      if r % $M == 0 then for i, v in ipairs(t) do if v % 2 == 0 then break end s = s + 1 end end
    end
  end
  return s
end
local function flip$id(rounds)
  local t = {1, 2, 3, 4, w = 5}
  local calls = 0
  local mt = { __index = function(_, i) if i < $MAXN then return i * 3 end end,
               __pairs = function(tt) calls = calls + 1 return next, tt, nil end }
  local s = 0
  for r = 1, rounds do
    if r % $M2 == 1 then setmetatable(t, mt) elseif r % $M2 == $M3 then setmetatable(t, nil) end
    for i, v in ipairs(t) do s = s + v end
    for k, v in pairs(t) do s = s + v end
    if r % $M4 == 0 then t[#t + 1] = r elseif r % $M4 == 1 and #t > 6 then t[#t] = nil end
  end
  return s, calls
end
local function shadow$id(rounds)
  local ip, pa = ipairs, pairs
  local s = 0
  for r = 1, rounds do
    if r == $M5 then ipairs = function(t) return ip({1, 1, 1}) end end
    if r == $M6 then pairs = function(t) return pa({[1] = 4}) end end
    local t = G$id[r % #G$id + 1]
    for i, v in ipairs(t) do s = s + v end
    for k, v in pairs(t) do s = s + v end
  end
  ipairs, pairs = ip, pa
  return s
end
local function results$id(rounds)
  local t = G$id[$NT]
  local s = 0
  for r = 1, rounds do
    local a, b, c = ipairs(t)
    local p, q, w = pairs(t)
    s = s + select('#', ipairs(t)) * 10 + select('#', pairs(t)) + (b == t and 1 or 0) + (c == 0 and 1 or 0) + (w == nil and 1 or 0)
    for i, v in a, b, c do s = s + i end
    for k, v in p, q, w do s = s + 1 end
    local ok, e = pcall(function() for i, v in ipairs(r % 7 == 0 and nil or t) do s = s + 1 end end)
    if not ok then s = s + #e end
    ok, e = pcall(function() for k, v in pairs(r % 11 == 0 and 5 or t) do s = s + 1 end end)
    if not ok then s = s + #e end
  end
  return s
end
local function all$id()
  mix(nest$id($N))
  mix(flip$id($N))
  mix(shadow$id($N))
  mix(results$id($N))
  mix(nest$id($N))
end
run("g$id", all$id)
]==], { id = id, NT = rnd(6, 14), A = rnd(1, 7), B = rnd(2, 9), MAXN = rnd(3, 30), M = rnd(3, 9), M2 = rnd(7, 15), M3 = rnd(3, 6), M4 = rnd(4, 9), M5 = rnd(20, 60), M6 = rnd(60, 120), N = N(80, 240) })
end

-- N3: metamethods that are Lua closures called from hot loops: arithmetic (register / constant / immediate operands, both orders),
-- unary, `#`, `..`, `==` `<` `<=` (and the immediates), results that are not booleans or missing, the metamethod, the metatable or the
-- operand kind changing in the middle of a run, errors raised inside a body, a yield inside a body, nested metamethods
TT[#TT + 1] = function(id)
  return tpl([==[
local M$id = {}
M$id.__add = function(a, b) return (type(a) == "table" and a.v or a) + (type(b) == "table" and b.v or b) end
M$id.__sub = function(a, b) return (type(a) == "table" and a.v or a) - (type(b) == "table" and b.v or b) end
M$id.__mul = function(a, b) return (type(a) == "table" and a.v or a) * (type(b) == "table" and b.v or b) end
M$id.__unm = function(a) return -a.v end
M$id.__len = function(a) return a.v % 7 end
M$id.__concat = function(a, b) return (type(a) == "table" and "m" .. a.v or a) .. (type(b) == "table" and "m" .. b.v or b) end
M$id.__eq = function(a, b) return a.v % $K == b.v % $K end
M$id.__lt = function(a, b) return (type(a) == "table" and a.v or a) < (type(b) == "table" and b.v or b) end
M$id.__le = function(a, b) return (type(a) == "table" and a.v or a) <= (type(b) == "table" and b.v or b) end
local function mk$id(v) return setmetatable({ v = v }, M$id) end
local O$id = {}
for i = 1, 6 do O$id[i] = mk$id(i * $A) end
local function arith$id(rounds)
  local s = 0
  for r = 1, rounds do
    local a, b = O$id[r % 6 + 1], O$id[(r * 3) % 6 + 1]
    s = s + (a + b) + (a - b) * 2 + a * 3 + 5 - b + (-a) + #b + (a + 1.5) + (2 * b)
    local c = a .. b
    s = s + #c + #(a .. "z") + #("z" .. b)
    if a == b then s = s + 1 end
    if a ~= b then s = s + 2 end
    if a < b then s = s + 4 end
    if a <= b then s = s + 8 end
    if a > 10 then s = s + 16 end
    if a >= $B then s = s + 32 end
    if 20 < b then s = s + 64 end
    if b <= 3 then s = s + 128 end
    if r == $N2 then M$id.__add = function(x, y) return 1 end end
    if r == $N3 then M$id.__lt = function(x, y) return false end end
    if r == $N4 then setmetatable(O$id[2], nil) end
    if r == $N5 then M$id.__eq = nil end
  end
  return s
end
local function reset$id()
  M$id.__add = function(a, b) return (type(a) == "table" and a.v or a) + (type(b) == "table" and b.v or b) end
  M$id.__lt = function(a, b) return (type(a) == "table" and a.v or a) < (type(b) == "table" and b.v or b) end
  M$id.__eq = function(a, b) return a.v % $K == b.v % $K end
  setmetatable(O$id[2], M$id)
end
local function errs$id(rounds)
  local s = 0
  for r = 1, rounds do
    local a = O$id[r % 6 + 1]
    if r == $N2 then a = { v = 1 } end
    local ok, e = pcall(function() return a + 1 end)
    s = s + (ok and e or #tostring(e))
    if r == $N3 then O$id[1].v = nil end
    ok, e = pcall(function() return O$id[1] * 2 end)
    s = s + (ok and e or 1)
    O$id[1].v = 1 * $A
  end
  return s
end
local function yields$id(rounds)
  local Y = { __add = function(a, b) if b % $K == 0 then coroutine.yield(b) end return b + 1 end }
  local y = setmetatable({}, Y)
  local co = coroutine.create(function() local s = 0 for i = 1, rounds do s = s + (y + i) end return s end)
  local got = 0
  local ok, v = coroutine.resume(co)
  while coroutine.status(co) == "suspended" do
    got = got + v
    ok, v = coroutine.resume(co)
  end
  return got + (ok and v or -1)
end
local function nested$id(rounds)
  local A = setmetatable({}, { __add = function(a, b) return b * 2 end })
  local B = setmetatable({}, { __add = function(a, b) return (A + b) + 1 end })
  local s = 0
  for r = 1, rounds do s = s + (B + r) end
  return s
end
local function all$id()
  mix(arith$id($N))
  reset$id()
  mix(arith$id($N))
  reset$id()
  mix(errs$id($N))
  mix(yields$id($N))
  mix(nested$id($N))
end
run("m$id", all$id)
]==], { id = id, A = rnd(1, 9), B = rnd(5, 40), K = rnd(2, 5), N2 = rnd(20, 60), N3 = rnd(60, 100), N4 = rnd(100, 140), N5 = rnd(140, 180), N = N(120, 260) })
end

-- N3b: `setmetatable(t, mt)` in hot loops: a fresh table with a metatable (plain call and `return setmetatable(..)` tail call, also from a
-- metamethod body), a re-set object, nil / protected / `__gc` metatables appearing in the middle of a run, wrong argument kinds
-- (the interpreter's error texts), the returned table, a metamethod that allocates (collector steps between iterations)
TT[#TT + 1] = function(id)
  return tpl([==[
local V$id = {}
V$id.__index = V$id
V$id.__add = function(a, b) return setmetatable({ x = a.x + b.x, y = a.y + b.y }, V$id) end
V$id.__eq = function(a, b) return a.x == b.x and a.y == b.y end
function V$id.sum(o) return o.x + o.y end
local W$id = { __index = { x = $A, y = $B, w = 5 } }
local function new$id(x, y) return setmetatable({ x = x, y = y }, V$id) end
local function tail$id(x, y) return setmetatable({ x = x, y = y }, V$id) end
local function build$id(rounds)
  local s = 0
  local acc = new$id(0, 0)
  local one = tail$id(1, $A)
  for r = 1, rounds do
    acc = acc + one
    local o = new$id(r, $B)
    local q = setmetatable({ x = r }, W$id)
    s = s + o:sum() + q.y + q.w + (acc == one and 1 or 0) + (getmetatable(o) == V$id and 2 or 0)
    if r == $N2 then W$id.__index = { x = 1, y = 2, w = 3 } end
    if r == $N3 then V$id.sum = function(o) return o.x - o.y end end
  end
  return s + acc.x * 3 + acc.y
end
local function reset$id()
  W$id.__index = { x = $A, y = $B, w = 5 }
  V$id.sum = function(o) return o.x + o.y end
end
local function again$id(rounds)
  local t = { x = 1, y = 2 }
  local s = 0
  for r = 1, rounds do
    local m = r % 3 == 0 and nil or (r % 3 == 1 and V$id or W$id)
    local back = setmetatable(t, m)
    if back ~= t then s = s + 1000 end
    s = s + (t.w or 0) + (rawget(t, "x") or 0)
  end
  return s
end
local function kinds$id(rounds)
  local s = 0
  local P = { __metatable = "locked" }
  local G = {}
  local fin = 0
  local objs = {}
  for i = 1, 50 do objs[i] = {} end
  for r = 1, rounds do
    local o = objs[r % 50 + 1]
    if r == $N3 then setmetatable(objs[7], P) end
    local ok, e = pcall(setmetatable, o, V$id)
    s = s + (ok and 1 or #e)
    if r == $N4 then G.__gc = function() fin = fin + 1 end end
    setmetatable({}, G)
    local ok2, e2 = pcall(setmetatable, r % 7 == 0 and 5 or {}, r % 11 == 0 and 3 or nil)
    s = s + (ok2 and 0 or #e2)
  end
  collectgarbage() collectgarbage()
  return s + fin * 1000
end
local function all$id()
  mix(build$id($N))
  reset$id()
  mix(build$id($N))
  reset$id()
  mix(again$id($N))
  mix(kinds$id($N))
end
run("sm$id", all$id)
]==], { id = id, A = rnd(1, 9), B = rnd(2, 20), N2 = rnd(20, 60), N3 = rnd(60, 100), N4 = rnd(100, 140), N = N(120, 260) })
end

-- R1: tail calls in hot loops (inline `TAILCALL` of tier 1): self and mutual tail recursion, a tail call with too few / too many
-- arguments, to a vararg function, to a C function and to a `__call` table, a callee that changes in the middle of a run, deep tail
-- recursion (stack reuse), a callee with many registers called while the stack is small, errors and tracebacks through tail frames,
-- loops with calls to C functions in their bodies (the loop-entry rule of the back-off monitor), a hook set in the middle of a run
TT[#TT + 1] = function(id)
  return tpl([==[
local function loop$id(n, acc) if n == 0 then return acc end return loop$id(n - 1, acc + n) end
local ev$id, od$id
function ev$id(n) if n == 0 then return true end return od$id(n - 1) end
function od$id(n) if n == 0 then return false end return ev$id(n - 1) end
local function c3$id(a, b, d) if d == nil then return a + b end return a + b + d end
local function few$id(x) return c3$id(x, $A) end
local function many$id(x) return c3$id(x, $A, 2, 3) end
local function va$id(...) return select('#', ...) + (... or 0) end
local function tva$id(x) return va$id(x, $A) end
local function tc$id(x) return tostring(x) end
local function tm$id(x) return math.max(x, $B) end
local callable$id = setmetatable({}, { __call = function(self, a) return a * $K end })
local function tcall$id(x) return callable$id(x) end
local function capt$id(x)
  local y = x
  local f = function() return y end
  return c3$id(y, f(), $A)
end
local target$id = function(a) return a + 1 end
local function tt$id(x) return target$id(x) end
local function bad$id(x) if x == $N2 then error("boom " .. x) end return x end
local function tbad$id(x) return bad$id(x) end
local function tb$id() return debug.traceback("t", 1) end
local function tb2$id() return tb$id() end
local function run$id(n)
  local s = 0
  for i = 1, n do
    s = s + loop$id($K + 8, 0) + few$id(i) + many$id(i) + tva$id(i) + #tc$id(i) + tm$id(i) + tcall$id(i) + capt$id(i) + tt$id(i)
    if ev$id($K + i % 2) then s = s + 1 end
    if i == $N2 then target$id = function(a, b) return a * 2 end end
    if i == $N3 then target$id = callable$id end
    local ok, e = pcall(tbad$id, i)
    if not ok then s = s + #e end
  end
  return s
end
local function deep$id(n) return loop$id(n, 0) end
local function all$id()
  mix(run$id($N))
  target$id = function(a) return a + 1 end
  mix(run$id($N))
  mix(deep$id(30000))
  mix((tb2$id():gsub("0x%x+", "PTR")))
  local hits = 0
  debug.sethook(function() hits = hits + 1 end, "", 13)
  mix(run$id(50))
  debug.sethook()
  mix(hits > 0)
end
run("tf$id", all$id)
]==], { id = id, A = rnd(1, 9), B = rnd(2, 40), K = rnd(2, 5), N2 = rnd(20, 60), N3 = rnd(60, 100), N = N(120, 260) })
end

-- A1: constructors with constant string keys in hot loops (the inline `NEWREF` of tables the trace allocated): 3 to 8 keys (hash chains,
-- collisions), a key added after the constructor (rehash), a field set to nil and back, a field whose type changes in the middle of
-- an iteration, array and hash parts together, tables built to be metatables (the metamethod flags must not claim "absent"), a
-- metatable replaced in the middle of a run; every kept table is read back in sorted order (the node order depends on the hash seed)
TT[#TT + 1] = function(id)
  return tpl([==[
local function sorted$id(t)
  local ks = {}
  for k, v in pairs(t) do ks[#ks + 1] = tostring(k) .. "=" .. tostring(v) end
  table.sort(ks)
  return table.concat(ks, ",")
end
local lenf$id = function() return $A end
local base$id = { foo = $B }
local function shapes$id(n)
  local s, keep = 0, {}
  for i = 1, n do
    local a = { k1 = i, k2 = $A, k3 = $B }
    local b = { alpha = i, beta = $A, gamma = $B, delta = 4, eps = 5 }
    local c = { i, $A, x = i, y = $B }
    local d = { q1 = i, q2 = 2, q3 = 3, q4 = 4, q5 = 5, q6 = 6, q7 = 7, q8 = 8 }
    local e = { w = i % $K == 0 and 0.5 or i, z = $B }
    local m = { __index = base$id, __len = lenf$id }
    local o = setmetatable({}, m)
    a.k4 = i
    if i % 7 == 0 then b.eps = nil end
    if i % 11 == 0 then b.zeta = i end
    keep[i % 5 + 1] = d
    keep[i % 3 + 6] = m
    keep[9 + i % 2] = b
    s = s + a.k1 + a.k2 + a.k3 + a.k4 + b.alpha + b.gamma + (b.eps or 0) + c[1] + c.x + d.q1 + d.q8 + e.w + o.foo + #o
    if i == $N2 then base$id = { foo = $A } end
  end
  for j = 1, 5 do mix(sorted$id(keep[j])) end
  for j = 9, 10 do mix(sorted$id(keep[j])) end
  for j = 6, 8 do
    local oo = setmetatable({}, keep[j])
    mix(oo.foo)
    mix(#oo)
  end
  return s
end
local function all$id()
  mix(shapes$id($N))
  base$id = { foo = $B }
  mix(shapes$id($N))
  mix(shapes$id(9))
end
run("nr$id", all$id)
]==], { id = id, A = rnd(1, 9), B = rnd(2, 20), K = rnd(2, 5), N2 = rnd(20, 60), N = N(120, 260) })
end

-- S1: hot guard exits inside inlined frames (side traces with virtual frames): polymorphic helpers at depth 1 to 3 (branches on
-- the iteration number, three ways, argument type changes), tail frames of alternating length, closures of one prototype with
-- different upvalues, result adjustment, metamethod bodies that branch (arithmetic, comparison, `__index`, `__newindex`), an error in
-- a rare branch with a traceback, a helper that changes in the middle of a run, a hook set in the middle, and a run in a fresh
-- coroutine (small stack) whose side branch calls a function with many registers
TT[#TT + 1] = function(id)
  return tpl([==[
local function leaf$id(x) local m = x % $K if m == 0 then return x + $A elseif m == 1 then return x * 2 else return x - $B end end
local function mid$id(x) local v = leaf$id(x) + leaf$id(x + 1) if v % 3 == 0 then return v // 2 end return v end
local function top$id(x) return mid$id(x) + 1 end
local ev$id, od$id
function ev$id(n) if n == 0 then return true end return od$id(n - 1) end
function od$id(n) if n == 0 then return false end return ev$id(n - 1) end
local function mk$id(k)
  local acc = 0
  return function(x) acc = acc + 1 if x % 2 == 0 then return x + k + acc % 3 end return x - k end
end
local fs$id = { mk$id(1), mk$id($A), mk$id($B) }
local function adj$id(x) if x % 3 == 0 then return x, x + 1 elseif x % 3 == 1 then return x else return end end
local V$id = {}
V$id.__add = function(a, b) if a.x % 2 == 0 then return a.x + b.x else return a.x - b.x end end
V$id.__lt = function(a, b) if a.x % 3 == 0 then return a.x < b.x end return a.x + 1 == b.x end
V$id.__unm = function(a) if a.x % $K == 0 then return -a.x end return a.x end
local ma$id, mb$id = setmetatable({ x = 1 }, V$id), setmetatable({ x = $A }, V$id)
local ix$id = setmetatable({}, { __index = function(t, k) if k % 2 == 0 then return k end return -k end })
local sink$id = 0
local wi$id = setmetatable({}, { __newindex = function(t, k, v) if k % 2 == 0 then sink$id = sink$id + v else sink$id = sink$id - 1 end end })
local helper$id = function(x) if x % 2 == 0 then return x + $A end return x - 1 end
local function thr$id(x) if x % $N2 == 0 then error("boom " .. x) end return x end
local function mdl$id(x) return thr$id(x) + 1 end
local function run$id(n)
  local s = 0
  for i = 1, n do
    s = s + top$id(i) + fs$id[i % 3 + 1](i) + helper$id(i)
    if ev$id($K + i % 2) then s = s + 1 end
    local a, b = adj$id(i)
    s = s + (a or 0) + (b or 100) + select("#", adj$id(i))
    ma$id.x = i
    s = s + (ma$id + mb$id) + (-ma$id) + ix$id[i] + ix$id[i % 5 + 1]
    if ma$id < mb$id then s = s + 1 end
    wi$id[i] = i
    local v = (i % 4 == 0) and i + 0.5 or i
    s = s + (v == 0 and 0 or v + 1)
    if i == $N3 then helper$id = function(x) if x % 3 == 0 then return x end return x * 2 end end
    local ok, e = xpcall(mdl$id, function(m) return #debug.traceback(m, 1) end, i)
    s = s + (ok and e or e % 100)
  end
  return s
end
local function big$id()
  local src = { "return function(x)" }
  for j = 1, 90 do src[#src + 1] = "local v" .. j .. " = x + " .. j end
  src[#src + 1] = "return v1 + v45 + v90 end"
  return assert(load(table.concat(src, "\n")))()
end
local bg$id = big$id()
local function pick$id(x) if x % 2 == 0 then return bg$id(x) end return x end
local function co$id()
  local t = 0
  for r = 1, 4 do
    local c = coroutine.wrap(function()
      local u = 0
      for i = 1, $N do u = u + pick$id(i) + leaf$id(i) end
      return u
    end)
    t = t + c()
  end
  return t
end
local function all$id()
  mix(run$id($N))
  helper$id = function(x) if x % 2 == 0 then return x + $A end return x - 1 end
  mix(run$id($N))
  mix(sink$id)
  mix(co$id())
  local hits = 0
  debug.sethook(function() hits = hits + 1 end, "", 17)
  mix(run$id(60))
  debug.sethook()
  mix(hits > 0)
end
run("fs$id", all$id)
]==], { id = id, A = rnd(1, 9), B = rnd(2, 20), K = rnd(2, 5), N2 = rnd(40, 90), N3 = rnd(60, 100), N = N(120, 260) })
end

-- M1: a tree-walking interpreter: mutually recursive eval / exec with generic `for` (the close flag `k` on every RETURN and
-- TAILCALL), nodes with nil array elements, closures capturing a local of a returning frame (open upvalue at the RETURN),
-- errors with tracebacks from inside the recursion, a node kind that appears in the middle of the run, a coroutine that yields
-- from the recursion, a hook set in the middle
TT[#TT + 1] = function(id)
  return tpl([==[
local ev$id, ex$id
local function nd$id(k, a, b, c) return { k, a, b, c } end
function ev$id(e, env)
  local k = e[1]
  if k == 1 then return e[2]
  elseif k == 2 then return env[e[2]] or 0
  elseif k == 3 then
    local l, r = ev$id(e[3], env), ev$id(e[4], env)
    if e[2] == 1 then return l + r elseif e[2] == 2 then return l * r else return l - r end
  elseif k == 4 then
    local s = 0
    for _, x in ipairs(e[2]) do s = s + ev$id(x, env) end
    return s
  elseif k == 5 then return ex$id(e[2], env)
  elseif k == 6 then
    local cap = e[2]
    local function g() cap = cap + 1; return cap end
    for _, v in ipairs({ 1 }) do cap = cap + v end
    if e[3] then return e[3](g) end
    return g() + g()
  elseif k == 7 then error("boom " .. tostring(e[2]))
  end
  return 0
end
function ex$id(stmts, env)
  for i = 1, #stmts do
    local s = stmts[i]
    local k = s[1]
    if k == 1 then env[s[2]] = ev$id(s[3], env)
    elseif k == 2 then
      local r
      if ev$id(s[2], env) ~= 0 then r = ex$id(s[3], env) elseif s[4] then r = ex$id(s[4], env) end
      if r then return r end
    elseif k == 3 then
      local n = 0
      while ev$id(s[2], env) ~= 0 and n < 50 do
        local r = ex$id(s[3], env)
        if r then return r end
        n = n + 1
      end
    elseif k == 4 then return ev$id(s[2], env)
    end
  end
end
local function num$id(v) return nd$id(1, v) end
local function var$id(n) return nd$id(2, n) end
local function bin$id(op, l, r) return nd$id(3, op, l, r) end
local function set$id(n, e) return nd$id(1, n, e) end
local function if$id(c, t, e) return nd$id(2, c, t, e) end
local prog$id = {
  set$id("i", num$id(0)), set$id("a", num$id($A)),
  nd$id(3, bin$id(3, num$id(20 + $K), var$id("i")), {
    set$id("a", bin$id(1, var$id("a"), bin$id(2, var$id("i"), num$id(3)))),
    if$id(bin$id(3, bin$id(3, var$id("i"), num$id(5)), num$id(1)), { set$id("a", bin$id(1, var$id("a"), num$id($B))) }),
    if$id(bin$id(3, var$id("i"), num$id(9)), { set$id("a", bin$id(3, var$id("a"), num$id(1))) },
      { set$id("a", bin$id(1, var$id("a"), num$id(2))) }),
    set$id("i", bin$id(1, var$id("i"), num$id(1))),
  }),
  nd$id(4, nd$id(4, { var$id("a"), var$id("i"), bin$id(2, num$id(2), var$id("a")) })),
}
local function run$id(n)
  local s = 0
  for r = 1, n do
    s = s + ex$id(prog$id, {})
    s = s + ev$id(nd$id(6, r, nil), {}) + ev$id(nd$id(6, r, function(g) return g() - g() end), {})
    local ok, m = pcall(ev$id, nd$id(5, { nd$id(4, nd$id(4, { nd$id(1, 1), nd$id(7, r % 5 == 0 and r or nil) })) }), {})
    s = s + (ok and 1 or #m)
  end
  return s
end
local function all$id()
  mix(run$id($N))
  prog$id[2] = nd$id(1, "a", nd$id(1, $B))
  mix(run$id($N))
  local co = coroutine.wrap(function()
    for r = 1, 30 do coroutine.yield(ex$id(prog$id, {})) end
    return 0
  end)
  local t = 0
  for r = 1, 30 do t = t + co() end
  mix(t)
  local hits = 0
  debug.sethook(function() hits = hits + 1 end, "", 17)
  mix(run$id(20))
  debug.sethook()
  mix(hits > 0)
end
run("m1$id", all$id)
]==], { id = id, A = rnd(1, 9), B = rnd(2, 20), K = rnd(2, 5), N = N(80, 160) })
end

-- M1: `ipairs` / `pairs` loops of inlined callees (unrolled by the recorder): lengths 0 / 1 / 2 / 5 / past the unroll bound, holes, a hash
-- continuation, tables that grow or shrink during the traversal, `break` / `return` inside the loop, nested loops, `__index` proxies, a
-- table that changes shape in the middle of the run, an error in the body, a yield inside the loop, a hook set in the middle
TT[#TT + 1] = function(id)
  return tpl([==[
local function sm$id(t) local s = 0 for _, v in ipairs(t) do s = s + v end return s end
local function fo$id(t, lim) for i, v in ipairs(t) do if v > lim then return i, v end end return nil end
local function br$id(t) local n = 0 for _, v in ipairs(t) do if v == $K then break end n = n + 1 end return n end
local function ne$id(a, b) local s = 0 for _, x in ipairs(a) do for _, y in ipairs(b) do s = s + x * y end end return s end
local function ks$id(t) local s = 0 for k, v in pairs(t) do s = s + #k + v end return s end
local function gr$id(t) local n = 0 for i, v in ipairs(t) do n = n + v if i == 2 and #t < 6 then t[#t + 1] = $A end end return n end
local function sh$id(t) local n = 0 for i, v in ipairs(t) do n = n + v if i == 1 then t[3] = nil end end return n end
local function bad$id(t) for _, v in ipairs(t) do if v == $B then error("b" .. v) end end return 0 end
local sets$id = { {}, { 1 }, { 1, 2 }, { 1, 2, 3, 4, 5 }, { 1, 2, nil, 4 }, { $K, $K, $K },
  (function() local t = {} for i = 1, $N2 do t[i] = i % 5 end return t end)(),
  (function() local t = { 1, 2 } t[3] = 3 t[4] = 4 return t end)() }
local px$id = setmetatable({}, { __index = function(_, i) if i <= 3 then return i * $A end end })
local function run$id(n)
  local acc = 0
  for r = 1, n do
    for k = 1, #sets$id + 1 do
      local t = sets$id[k] or {}
      acc = acc + sm$id(t) + br$id(t)
      local i, v = fo$id(t, 2)
      acc = acc + (i or -1) + (v or 0)
    end
    acc = acc + ne$id(sets$id[3], sets$id[4]) + ne$id({}, sets$id[4]) + ks$id({ a = 1, bb = 2, ccc = r % 4 })
    local g = { 1, 2 }
    acc = acc + gr$id(g) + #g + sh$id({ 5, 6, 7, 8 })
    acc = acc + sm$id(px$id)
    local ok, m = pcall(bad$id, r % 9 == 0 and { 1, $B } or { 1, 2 })
    if not ok then acc = acc + #m end
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  sets$id[4] = { 9, 8, 7 }
  sets$id[2] = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20 }
  mix(run$id($N))
  local co = coroutine.wrap(function()
    for r = 1, 30 do local s = 0 for _, v in ipairs({ 1, 2, 3 }) do s = s + coroutine.yield(v) end end
    return 0
  end)
  local t = 0
  for r = 1, 90 do t = t + (co(1) or 0) end
  mix(t)
  local hits = 0
  debug.sethook(function() hits = hits + 1 end, "", 17)
  mix(run$id(10))
  debug.sethook()
  mix(hits > 0)
end
run("m1g$id", all$id)
]==], { id = id, A = rnd(1, 9), B = rnd(3, 20), K = rnd(2, 5), N2 = rnd(10, 40), N = N(60, 140) })
end

-- F1: `x ^ 2`, `math.sin` / `cos` / `exp` / `log`, `math.min` / `math.max` of three or more arguments, `string.len` / `s:len()` / `#s` of short and long
-- strings (the function and the string metatable replaced in the middle of the run), and `pairs` / `next` traversals with assignment to existing
-- fields, interleaved traversals, nested loops and a table that changes in the middle of the run
TT[#TT + 1] = function(id)
  return tpl([==[
local sl$id = "ab$K" .. string.rep("c", $A)
local sb$id = string.rep("z", $B * 9)
local function ar$id(n)
  local a = 0.0
  for i = 1, n do
    local x = i * 0.25 - $A
    a = a + x ^ 2 + (i % $K) ^ 2 + (x ^ 3) % 7 + math.sin(x) + math.cos(i) + math.exp(x / 50) + math.log(i)
    a = a + math.max(i, $B, 7) + math.min(i, 50, $K, 99) + math.max(x, 1.5, 2.5) + math.min(x, 0.5, -x, 9.0)
  end
  return a
end
local function sn$id(n)
  local a = 0
  for i = 1, n do
    local t = (i % 3 == 0) and sb$id or sl$id
    a = a + #t + t:len() + string.len(t) + string.len(sl$id)
  end
  return a
end
local tb$id = {}
for i = 1, $K * 4 do tb$id["w" .. i] = i; tb$id[i + 0.5] = -i end
local function pr$id(n)
  local s = 0
  for r = 1, n do
    for k, v in pairs(tb$id) do
      s = s + v
      if r % 4 == 0 then tb$id[k] = v + 1 end
      if r % 7 == 0 and v > $B then tb$id[k] = nil end
    end
    local k1, k2, v1, v2
    k1, v1 = next(tb$id)
    k2, v2 = next(tb$id, k1)
    s = s + (v1 and 1 or 0) + (v2 and 1 or 0) + (k1 ~= k2 and 1 or 0) -- (the first keys depend on the string hash seed: no values)
    for k in pairs(tb$id) do for j in next, tb$id do s = s + 1 end break end
  end
  return s
end
local function all$id()
  mix(ar$id($N)); mix(sn$id($N)); mix(pr$id($N))
  tb$id.extra = 5; tb$id[1000] = 6; tb$id[true] = 1
  mix(pr$id($N)); mix(sn$id($N))
  local sl = string.len
  string.len = function(x) return 11 end
  mix(sn$id($N))
  string.len = sl
  local mt = getmetatable("")
  local old = mt.__index
  mt.__index = { len = function() return 3 end }
  mix(sn$id($N))
  mt.__index = old
  mix(sn$id($N)); mix(ar$id($N))
  mix(pcall(string.len)); mix(pcall(next, tb$id, "nokey"))
end
run("f1$id", all$id)
]==], { id = id, A = rnd(1, 9), B = rnd(3, 20), K = rnd(2, 5), N = N(60, 140) })
end

-- R2: native recursion of methods / globals with loops and `RETURN0`, call sites that want 0 / 1 / 2 results from callees that return fewer,
-- a recursion root called from C frames (pcall, coroutine body, xpcall: stale `CallInfo` nodes), errors and yields from the recursion,
-- nil-slot stores (a field set to nil and set again, nil stores to absent keys), and a function replaced in the middle of the run
TT[#TT + 1] = function(id)
  return tpl([==[
local P$id = {}
P$id.__index = P$id
function P$id:sw(i, j) local t = self.v[i + 1]; self.v[i + 1] = self.v[j + 1]; self.v[j + 1] = t end
function P$id:pm(n)
  self.count = self.count + 1
  if n ~= 0 then
    local n1 = n - 1
    self:pm(n1)
    for i = n1, 0, -1 do
      self:sw(n1, i)
      self:pm(n1)
      self:sw(n1, i)
    end
  end
end
function hanoi$id(n, a, b, c) if n == 0 then return end hanoi$id(n - 1, a, c, b); M$id.n = M$id.n + a; hanoi$id(n - 1, c, b, a) end
M$id = { n = 0 }
local function f1$id(n) if n == 0 then return $A end local a, b = f1$id(n - 1); assert(b == nil); return a + 1 end
local function g0$id(n) if n == 0 then return end local x, y = g0$id(n - 1); assert(x == nil and y == nil) end
local function cn$id(...) return select("#", ...) end
local function m1$id(n) if n == 0 then return 0 end return cn$id(m1$id(n - 1)) + cn$id(m1$id(n - 1), 1, 2) + n end
local function fb$id(n) if n < 2 then return n end return fb$id(n - 1) + fb$id(n - 2) end
local Q$id = {}
Q$id.__index = Q$id
local function nq$id() return setmetatable({ head = nil, tail = nil, n = 0 }, Q$id) end
function Q$id:push(v)
  local node = { v = v, nxt = nil }
  if self.tail then self.tail.nxt = node else self.head = node end
  self.tail = node
  self.n = self.n + 1
end
function Q$id:pop()
  local h = self.head
  if h == nil then return nil end
  self.head = h.nxt
  if self.head == nil then self.tail = nil end
  h.nxt = nil
  self.n = self.n - 1
  return h.v
end
local function run$id(n)
  local acc = 0
  for r = 1, n do
    local p = setmetatable({ count = 0, v = { 0, 0, 0, 0, 0, 0 } }, P$id)
    p:pm(r % 4 + 2)
    acc = acc + p.count + p.v[1] + p.v[6]
    M$id.n = 0; hanoi$id(r % 5 + 1, 1, 2, 3); acc = acc + M$id.n
    acc = acc + f1$id(r % 6 + 1) + (g0$id(r % 4) == nil and 1 or 0) + m1$id(r % 4)
    local ok, v = pcall(fb$id, 7 + r % 3)
    acc = acc + v
    acc = acc + select(2, xpcall(fb$id, print, 5 + r % 4))
    local co = coroutine.wrap(function(k) return fb$id(k) end)
    acc = acc + co(6)
    local q = nq$id()
    for i = 1, 12 do q:push(i); if i % 3 == 0 then acc = acc + q:pop() + (q:pop() or 0) end end
    while q.head do acc = acc + q:pop() end
    q.scratch = nil; q.scratch = r; q.scratch = nil
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  local sw = P$id.sw
  P$id.sw = function(self, i, j) self.v[i + 1], self.v[j + 1] = self.v[j + 1], self.v[i + 1] end
  mix(run$id($N))
  P$id.sw = sw
  local function deep(n) return deep(n + 1) + 1 end
  local ok, e = pcall(deep, 1)
  mix(ok); mix(#tostring(e) > 0)
  local co = coroutine.wrap(function()
    local function y(n) if n == 0 then coroutine.yield(1) return 0 end local r = y(n - 1); return r + 1 end
    for i = 1, 20 do y(i % 7) end
    return 0
  end)
  local t = 0
  for i = 1, 25 do t = t + (co() or 0) end
  mix(t)
  local hits = 0
  debug.sethook(function() hits = hits + 1 end, "", 23)
  mix(run$id(6))
  debug.sethook()
  mix(hits > 0)
end
run("r2$id", all$id)
]==], { id = id, A = rnd(1, 9), N = N(25, 60) })
end

-- B2: scaled array addressing (variable indexes, every element type, 64-bit constants), array reads of non-nil elements while a metatable
-- with `__index` / `__newindex` appears later and elements become nil, and node tags written back with the same or another type
-- (aliased tables, two fields of one object)
TT[#TT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local f, iv, sv, bv, tv, w = {}, {}, {}, {}, {}, {}
  for i = 1, $M do f[i] = i * 0.25; iv[i] = i; sv[i] = "s" .. (i % 7); bv[i] = (i % 3 == 0); tv[i] = { i }; w[i] = 0 end
  local acc, hits = 0, 0
  local mt = { __index = function(t, k) hits = hits + 1; return -k end, __newindex = function(t, k, v) hits = hits + 1; rawset(t, k, v) end }
  local o = { x = $A, y = 2.5, s = "a", b = true }
  local a = { x = 1 }
  local b = a
  for r = 1, n do
    if r == n // 3 then setmetatable(iv, mt); setmetatable(w, mt) end
    if r == n // 2 then iv[3] = nil; w[4] = nil end
    if r == (2 * n) // 3 then iv[3] = 9 end
    for i = 1, $M do
      acc = acc + f[i] + iv[i] + #sv[i] + tv[i][1]
      if bv[i] then acc = acc + 1 end
    end
    for i = 1, $M do w[$M + 1 - i] = 1099511627776 + i + r end
    for i = 1, $M do if i % 2 == 0 then w[i] = i else w[i] = 0.5 + i end end
    o.x = o.x + 1
    o.y = o.y * 1.001
    if r == n // 4 then o.x = 0.5 end
    if r == n // 2 then o.x = "7" end
    acc = acc + (type(o.x) == "number" and o.x or 0)
    local v = a.x
    b.x = 2.5
    a.x = v
    acc = acc + a.x
  end
  for i = 1, $M do acc = acc + w[i] end
  return acc + hits
end
local function all$id()
  mix(run$id($N))
  mix(run$id($N + 7))
end
run("b2$id", all$id)
]==], { id = id, A = rnd(1, 9), M = N(12, 40), N = N(20, 60) })
end

-- F2: library calls recorded in traces: string comparisons (long `==`, `<`, `<=`), getmetatable (a protected metatable appears), rawget / rawset
-- through __index / __newindex tables, assert (a falsy value late), string.byte (positions, ranges, past the end), math.random (integer forms)
TT[#TT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local A, B, T, objs = {}, {}, {}, {}
  local mt1, mt2 = {}, { __index = function(t, k) return k end }
  for i = 1, $M do
    A[i] = string.rep("a", 41) .. (i % 5)
    B[i] = string.rep("a", 41) .. ((i + $A) % 5)
    T[i] = i
    objs[i] = setmetatable({}, (i % 3 == 0) and mt2 or mt1)
  end
  setmetatable(T, { __index = function(t, k) return -k end, __newindex = function(t, k, v) rawset(t, k, v + 1) end })
  local acc = 0
  math.randomseed($A)
  for r = 1, n do
    for i = 1, $M do
      if A[i] == B[i] then acc = acc + 1 end
      if A[i] < B[i] then acc = acc + 2 end
      if A[i] >= B[i] then acc = acc + 3 end
      local m = getmetatable(objs[i])
      if m == mt1 then acc = acc + 1 elseif m == mt2 then acc = acc + 2 else acc = acc + 3 end
      rawset(T, i, r + i)
      acc = acc + (rawget(T, i) or 0) + (rawget(T, i + $M) or 0)
      acc = acc + assert(T[i], "never") + select("#", assert(i, r, acc))
      local b1, b2 = string.byte(A[i], 1 + r % 3, 2 + r % 3)
      acc = acc + (b1 or 0) + (b2 or 0) + (string.byte(A[i], -1) or 0) + (string.byte(A[i], 60) or 0)
      acc = acc + math.random(1, 9) % 2 + math.random(i)
      if r == n // 2 and i == $M then mt1.__metatable = "locked"; B[1] = A[1] end
      if r == (3 * n) // 4 and i == 2 then T[i] = nil end
    end
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  mix(select(2, pcall(function() for i = 1, 40 do assert(i < 30 or nil, "late " .. i) end end)))
  mix(select(2, pcall(run$id, 0)))
end
run("f2$id", all$id)
]==], { id = id, A = rnd(1, 4), M = N(8, 24), N = N(20, 50) })
end

-- C2: tier-1 comparisons of non-numbers (strings, tables with and without `__eq`, functions, booleans) in loops whose operand kinds change,
-- error comparisons through pcall, and `SETUPVAL` of collectable values (closed upvalues) with collections in between
TT[#TT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local eqmt = { __eq = function(a, b) return a.v == b.v end, __lt = function(a, b) return a.v < b.v end, __le = function(a, b) return a.v <= b.v end }
  local plain = { __index = function(t, k) return k end }
  local vals = {}
  for i = 1, $M do
    local k = (i + $A) % 6
    if k == 0 then vals[i] = "s" .. (i % 5)
    elseif k == 1 then vals[i] = string.rep("l", 41) .. (i % 3)
    elseif k == 2 then vals[i] = setmetatable({ v = i % 4 }, eqmt)
    elseif k == 3 then vals[i] = setmetatable({ v = i % 4 }, plain)
    elseif k == 4 then vals[i] = (i % 2 == 0) and print or tostring
    else vals[i] = i * 0.5 end
  end
  local up = {}
  local function setup(v) up = v end
  local acc = 0
  for r = 1, n do
    for i = 1, $M do
      local a, b = vals[i], vals[i % $M + 1]
      if a == b then acc = acc + 1 end
      if a ~= vals[(i + r) % $M + 1] then acc = acc + 2 end
      local ok, res = pcall(function() return a < b end)
      if ok then if res then acc = acc + 4 end else acc = acc + 8 end
      ok, res = pcall(function() return a <= b end)
      if ok then if res then acc = acc + 16 end else acc = acc + 32 end
      setup(a)
      if i % 7 == 0 then setup({ r, i }) end
    end
    if r == n // 3 then eqmt.__eq = nil end
    if r == n // 2 then eqmt.__eq = function() return true end end
    if r == (2 * n) // 3 then collectgarbage("step", 0); vals[2] = "s0"; vals[3] = vals[2] end
    acc = acc + (type(up) == "table" and (up[1] or 0) or 1)
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  mix(run$id($N + 5))
end
run("c2$id", all$id)
]==], { id = id, A = rnd(0, 5), M = N(8, 24), N = N(20, 50) })
end

-- T3: the inline first `__index` level of tier 1 (`GETFIELD` / `SELF` / `GETTABUP`): instances of a 1..3-level class chain, a metatable swapped,
-- `__index` replaced by a function / nil / another table, a dead key in a class, a field only in the metatable, an `_ENV` with an
-- `__index` table, errors through a miss
TT[#TT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local A = { a = 1, who = function() return "A" end }
  A.__index = A
  local B = setmetatable({ b = 2, who = function() return "B" end }, A)
  B.__index = B
  local C = setmetatable({ c = 3, inmt = "m" }, B)
  C.__index = { cx = 4 }
  local inst = { setmetatable({ own = $A }, A), setmetatable({ own = 2 }, B), setmetatable({ own = 3 }, C), setmetatable({}, { __index = A })}
  local ge = load("local s = 0 for i = 1, 20 do s = s + (genva or 1) + (genvb or 2) end return s", "t3", "t", setmetatable({}, { __index = { genva = 5 } }))
  local acc = 0
  for r = 1, n do
    for i = 1, #inst do
      local o = inst[i]
      acc = acc + (o.own or 0) + (o.a or 0) + (o.b or 0) + (o.c or 0) + (o.cx or 0) + (o.inmt and 1 or 0) + (o.zzz and 100 or 0)
      if o:who() == "B" then acc = acc + 7 end
    end
    acc = acc + ge()
    if r == n // 5 then B.a = nil; A.a = 11 end
    if r == n // 4 then setmetatable(inst[1], B) end
    if r == n // 3 then getmetatable(inst[4]).__index = function(_, k) return #k end end
    if r == n // 2 then getmetatable(inst[4]).__index = nil; B.who = nil end
    if r == (3 * n) // 5 then getmetatable(inst[4]).__index = B; rawset(inst[2], "a", 50); B.who = A.who end
    if r == (2 * n) // 3 then C.__index = A; inst[3].own = nil end
    if r % 9 == 0 then collectgarbage("step", 0) end
    local ok, e = pcall(function() return inst[r % 4 + 1].nofn() end)
    if not ok then acc = acc + #e % 5 end
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  mix(select(2, pcall(function() local x = nil; for i = 1, 30 do if i == 25 then return x.f end end end)))
end
run("t3$id", all$id)
]==], { id = id, A = rnd(1, 4), N = N(20, 50) })
end

-- P2: a scheduler loop whose body dispatches to several task kinds (class guards that fail in turn: every exit site gets a side trace, the tree
-- must not be flushed as ineffective) and a rolled loop (a table constructor) whose element type flips between phases (hot type-guard exits
-- get side traces instead of a re-record)
TT[#TT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local K = {}
  for k = 1, 4 do
    local c = {}
    c.__index = c
    c.run = function(self, v) self.n = self.n + k; return (v * $A + k) % 1009 end
    K[k] = c
  end
  local tasks = {}
  for k = 1, 4 do tasks[k] = setmetatable({ n = 0 }, K[k]) end
  local v, acc = 1, 0
  for r = 1, n do
    local t = tasks[(r // $B) % 4 + 1]
    if r % 5 == 0 then t = tasks[r % 4 + 1] end
    v = t:run(v)
    if v % 7 == 0 then acc = acc + 1 end
  end
  local flip = {}
  for i = 1, 40 do flip[i] = (i % $C == 0) and (i + 0.5) or i end
  local s = 0
  for rep = 1, 8 do
    local src = flip
    if rep > $D then src = {}; for i = 1, 40 do src[i] = i * 1.5 end end
    for i = 1, 40 do local o = { i }; s = s + src[i] * 2 + o[1] end
  end
  return v + acc + s + tasks[1].n + tasks[4].n
end
local function all$id()
  mix(run$id($N))
  mix(run$id($N + 3))
end
run("p2$id", all$id)
]==], { id = id, A = rnd(2, 9), B = rnd(1, 40), C = rnd(2, 5), D = rnd(1, 7), N = N(300, 900) })
end

-- BUNDLE-1: functions reached only by tail calls (chains, mutual, vararg, C callees, a callee swapped mid-loop, an error and a yield in the
-- callee, call hooks / getinfo seeing the tail frame) and string-length / byte / compare loops (peeled with `hdrlib`) while the string
-- metatable's `__index` table and `string.len` change
TT[#TT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local function leaf(x) if x % $A == 0 then return x + 1 end return x * 2 end
  local function mid(x) return leaf(x) end
  local function top(x) return mid(x + 1) end
  local ev, od
  function ev(k) if k == 0 then return 1 end return od(k - 1) end
  function od(k) if k == 0 then return 0 end return ev(k - 1) end
  local function va(...) return select("#", ...), ... end
  local function tva(a, b) return va(a, b, a + b) end
  local function boom(x) if x == $B then error("b" .. x) end return x end
  local function tb(x) return boom(x) end
  local function who() return debug.getinfo(1, "t").istailcall and 1 or 0 end
  local function tw() return who() end
  local cur = function(x) return x + 1 end
  local alt = function(x) return x * 3 end
  local function tc(x) return cur(x) end
  local co = coroutine.wrap(function() for i = 1, n do coroutine.yield(top(i)) end end)
  local str, long = "abcdef" .. $A, string.rep("xy", 25)
  local smt = getmetatable("")
  local keep = string.len
  local acc = 0
  for i = 1, n do
    acc = acc + top(i) + ev(i % 5) + select(1, tva(i, 2)) + tw()
    local ok = pcall(tb, i)
    if not ok then acc = acc + 3 end
    acc = acc + tc(i) + co()
    acc = acc + #str + #long + str:len() + long:byte(i % 50 + 1) + str:byte(-1) + (str == "abcdef$A" and 1 or 0)
    if long < str then acc = acc + 100 end
    if type(str) == "string" then acc = acc + 1 end
    if i == n // 4 then cur = alt end
    if i == n // 2 then smt.__index = { len = function() return 9 end, byte = string.byte } end
    if i == (3 * n) // 4 then smt.__index = string; string.len = function() return 4 end end
    if i == n - 2 then string.len = keep; cur = function(x) return x - 1 end end
  end
  local calls = 0
  debug.sethook(function() calls = calls + 1 end, "c")
  for i = 1, 20 do top(i) end
  debug.sethook()
  return acc + calls
end
local function all$id()
  mix(run$id($N))
  mix(select(2, pcall(function() local x = nil; for i = 1, 30 do if i == 25 then return x.f end end end)))
end
run("b1$id", all$id)
]==], { id = id, A = rnd(2, 5), B = rnd(5, 30), N = N(60, 200) })
end

-- B4-1: tag-only stores of nil / false / true over integers, floats, strings and tables (arrays and fields); equality, `next`, `#`,
-- unpack, table.move and the collector must see only the tag
TT[#TT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local a, o = {}, { x = 1, y = 2.5, z = "s", w = {} }
  for i = 1, $M do a[i] = i * 7919 end
  local acc = 0
  for r = 1, n do
    for i = 1, $M do a[i] = ((i + r) % $A == 0) end
    for i = 1, $M do if a[i] == true then acc = acc + 1 end end
    for i = 1, $M do a[i] = i * 31 + r end
    for i = 1, $M, 2 do a[i] = nil end
    for k, v in pairs(a) do acc = acc + v end
    for i = 1, $M do a[i] = i + 0.25 end
    for i = 1, $M do a[i] = false end
    for i = 1, $M do if rawequal(a[i], false) then acc = acc + 1 end end
    for i = 1, $M do a[i] = { i } end
    for i = 1, $M do a[i] = (i % 4 == 0) and nil or true end
    if r % 5 == 0 then collectgarbage() end
    for i = 1, $M do if a[i] == nil then acc = acc + 2 elseif a[i] then acc = acc + 3 end end
    local t2 = table.move(a, 1, $M, 1, {})
    acc = acc + #t2 % 7
    o.x = (r % 3 == 0); o.y = nil; o.z = true
    acc = acc + (o.x and 1 or 0) + (o.y == nil and 2 or 0) + (o.z and 4 or 0)
    o.x = r; o.y = r * 0.5; o.z = "t" .. (r % 4)
    acc = acc + o.x + o.y
    o.w = (r % 5 == 0) and false or {}
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  mix(run$id($N + 3))
end
run("b4a$id", all$id)
]==], { id = id, A = rnd(2, 5), M = N(12, 40), N = N(20, 50) })
end

-- B4-4: `//` and `%` by constants (magic-number division): positive non-power-of-two divisors of every size, dividends of both signs
-- including the edges, destination = operand register or not, float operands mixed in
TT[#TT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local mi, ma = math.mininteger, math.maxinteger
  local xs = { 0, 1, -1, $A, -$A, 7, -7, 1000, -1000, 65535, 2147483647, -2147483648, 4294967297, ma, ma - 1, mi, mi + 1, ma // 7 * 7, -(ma // 3 * 3) }
  local acc = 0
  for r = 1, n do
    for _, x in ipairs(xs) do
      local y = x + r
      acc = (acc + y // 3 + y % 3 + y // $B + y % $B + x // 1000003 + x % 1000003) % 1000000007
      local z = x
      z = z // 10; x = x % 10
      acc = acc + z + x
      acc = acc + (x * 4294967311) // 255 + (y * 3037000493) % 65537
      acc = acc + y // 4611686018427387903 + y % 4611686018427387905 + (y // 9) % 1000
    end
    acc = acc + (r * 0.5) // 3 + (r * 0.5) % 3
    acc = acc + (r - 20) // 6 + (r - 20) % 6 + (r * 77777) // -7 + (r * 77777) % -7
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  mix(select(2, pcall(function() local x = $A; for i = 1, 5 do if i == 4 then return x // 0 end end end)))
end
run("b4d$id", all$id)
]==], { id = id, A = rnd(2, 99), B = rnd(5, 4000), N = N(20, 60) })
end

-- B4-3: field loads whose key tag and value tag are checked together: fields that change type late, keys that move (rehash), missing
-- keys, same-shape objects with another key order, a field read right after a write of another type
TT[#TT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local objs = {}
  for i = 1, 5 do objs[i] = { x = i + 0.5, y = i * 2, vx = 0.25 * i, m = i, f = (i % 2 == 0), s = "n" .. i } end
  local shp = {}
  for i = 1, 4 do if i % 2 == 0 then shp[i] = { a = 1, b = 2, c = 3 } else shp[i] = { c = 3, b = 2, a = 1 } end end
  local acc = 0
  for r = 1, n do
    for i = 1, 5 do
      local o = objs[i]
      local dx = o.x - 1.5
      acc = acc + dx * o.vx + o.y * o.m
      if o.f then acc = acc + 1 end
      acc = acc + #o.s
      o.x = o.x + o.vx * 0.01
      o.m = o.m + 1
    end
    for i = 1, 4 do local t = shp[i]; acc = acc + t.a + t.b + t.c end
    if r == n // 5 then objs[2].m = 2.5 end
    if r == n // 4 then objs[3].y = 0.5 end
    if r == n // 3 then objs[4].vx = 3 end
    if r == n // 2 then objs[1].f = 0; objs[5].s = "a longer string field" end
    if r == (2 * n) // 3 then for i = 1, 5 do objs[i]["k" .. i] = i end; shp[1].a = 1.5 end
    if r == (3 * n) // 4 then objs[2].s = nil; objs[2].s = "back" end
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  mix(select(2, pcall(function() local o = { x = 1 }; for i = 1, 20 do if i == 15 then o.x = nil end; mix(o.x + 1) end end)))
end
run("b4h$id", all$id)
]==], { id = id, N = N(20, 60) })
end

-- B4-2: call-free hot loops (no poll in the loop body: the stay budget bounds them), the same loops entered again under a count / line
-- hook, a hook installed from inside a loop that calls natives, a short run entered with a hook pending
TT[#TT + 1] = function(id)
  return tpl([==[
local function sum$id(n, m)
  local s = 0
  for i = 1, n do s = s + i % m end
  return s
end
local function grid$id(n)
  local a, s = {}, 0
  for i = 1, n do a[i] = i * 2 end
  for r = 1, 3 do for i = 1, n do s = s + a[i] - r end end
  return s
end
local function run$id(n)
  local acc = 0
  for r = 1, 40 do acc = acc + sum$id(n + r, $A) + grid$id(30 + r) end
  local calls = 0
  debug.sethook(function() calls = calls + 1 end, "", $B)
  acc = acc + sum$id(n * 2, $A)
  debug.sethook()
  local lines = 0
  local t = {}
  for i = 1, n do
    t[i % 8 + 1] = tostring(i)
    if i == n // 2 then debug.sethook(function() lines = lines + 1 end, "l") end
    if i == n // 2 + 3 then debug.sethook() end
  end
  acc = acc + sum$id(n, $A + 1)
  return acc + (calls > 0 and 1 or 0) + (lines > 0 and 1 or 0)
end
local function all$id()
  mix(run$id($N))
end
run("b4p$id", all$id)
]==], { id = id, A = rnd(3, 9), B = rnd(50, 400), N = N(200, 800) })
end

-- B5-1 (`sinkmt`, `sinkside`): vector temporaries with a metatable that die inside the loop, branches (side traces) that keep them live,
-- escapes in some iterations, shapes that change, a `__gc` that appears late
TT[#TT + 1] = function(id)
  return tpl([==[
local V$id = {}
V$id.__index = V$id
local function vec$id(x, y) return setmetatable({ x = x, y = y }, V$id) end
function V$id:add(o) return vec$id(self.x + o.x, self.y + o.y) end
function V$id:dot(o) return self.x * o.x + self.y * o.y end
local function run$id(n)
  local a, b, keep = vec$id(1, $A), vec$id(0.5, 2), {}
  local acc, cnt = 0, 0
  for i = 1, n do
    local c = a:add(b)
    local d = vec$id(c.y, c.x):add(a)
    acc = acc + d:dot(c)
    if i % $M == 0 then acc = acc + d.x end
    if i % $K == 0 then keep[#keep + 1] = c end
    if i == n // 3 then b = vec$id(b.y, 3) end
    if i == n // 2 then V$id.__gc = false; V$id.extra = 1 end
    if i == (2 * n) // 3 then a = setmetatable({ x = 2, y = 1, z = 0 }, V$id) end
    if i == (3 * n) // 4 then V$id.__gc = function() cnt = cnt + 1 end end
    if i == (4 * n) // 5 then b = vec$id(0.25, 1) end
  end
  collectgarbage()
  return acc + #keep + cnt * 0
end
local function all$id()
  mix(run$id($N))
  mix(select(2, pcall(function() local t = vec$id(1, 2); for i = 1, 30 do if i == 25 then t = nil end; mix(t.x + 1) end end)))
end
run("b5s$id", all$id)
]==], { id = id, A = rnd(1, 5), M = rnd(3, 9), K = rnd(10, 40), N = N(200, 900) })
end
SINKT[#SINKT + 1] = #TT

-- BUNDLE-E (native returns of frameful side traces): a method that recurses inside a `for` loop (awfy queens), called from the loop, through
-- `pcall` (a fresh frame: the native-return guard fails) and from a coroutine; helper methods with branches that make frameful side traces
TT[#TT + 1] = function(id)
  return tpl([==[
local Q$id = {}
Q$id.__index = Q$id
function Q$id:get(r, c) return self.fr[r + 1] and self.fx[c + r + 1] and self.fn[c - r + $W] end
function Q$id:set(r, c, v) self.fr[r + 1] = v; self.fx[c + r + 1] = v; self.fn[c - r + $W] = v end
function Q$id:place(c)
  for r = 0, $W - 1 do
    if self:get(r, c) then
      self.rows[r + 1] = c
      self:set(r, c, false)
      if c == $W - 1 then return true end
      if self:place(c + 1) then return true end
      self:set(r, c, true)
    end
  end
  return false
end
local function queens$id(k)
  local q = setmetatable({ fr = {}, fx = {}, fn = {}, rows = {} }, Q$id)
  for i = 1, $W do q.fr[i] = true; q.rows[i] = -1 end
  for i = 1, 2 * $W do q.fx[i] = true; q.fn[i] = true end
  local ok
  if k % $P == 0 then local _; _, ok = pcall(q.place, q, 0)
  elseif k % $P == 1 then ok = coroutine.wrap(function() return q:place(0) end)()
  else ok = q:place(0) end
  local sig = 0
  for i = 1, $W do sig = sig * 10 + q.rows[i] end
  return ok and sig or -1
end
local function all$id()
  local acc = 0
  for it = 1, $N do acc = acc + queens$id(it) end
  mix(acc)
  mix(select(2, pcall(function() local q = setmetatable({ fr = {}, fx = {}, fn = {} , rows = {} }, Q$id); return q:place(0) end)))
end
run("be$id", all$id)
]==], { id = id, W = rnd(5, 8), P = rnd(3, 7), N = N(6, 40) })
end

-- BB-2 (`tset`): SETTABLE with a short-string key or an integer key in the hash part into existing nodes, stores into nil-valued nodes and empty
-- array slots, tables with `__newindex` added / removed late, key kinds that change at one store site, collectable values into old tables
TT[#TT + 1] = function(id)
  return tpl([==[
local function put$id(t, k, v) t[k] = v end
local function srt$id(t)
  local ks = {}
  for k, v in pairs(t) do ks[#ks + 1] = tostring(k) .. "=" .. tostring(v) end
  table.sort(ks)
  return table.concat(ks, ",")
end
local function run$id(n)
  local names, c, h, o = {}, {}, { [0] = 1, [-5] = 2, [1 << 33] = 3, [math.maxinteger] = 4, [$A] = 5 }, {}
  for i = 1, 12 do names[i] = "kk" .. i end
  names[13] = string.rep("w", 45); names[14] = 2.0; names[15] = 7; names[16] = 2.5; names[17] = true
  local ik = { 0, -5, 1 << 33, math.maxinteger, $A, 3, $B, -7, 1 << 50 }
  local nic, acc = 0, 0
  local mt = setmetatable({ x = 1, [$A] = 1 }, { __newindex = function(t, k, v) nic = nic + 1; rawset(t, k, v) end })
  local old = { s = false, [1 << 41] = false, [1] = false }
  -- BC: a table whose nodes hold nil (keys killed by a store of nil) and that gets a late `__newindex`: stores into those nodes
  -- must consult the metamethod (absent key), and a store of nil to an existing key must not
  local nn = { a = 1, b = 2, [$A] = 3, c = 4 }
  nn.a = nil; nn.b = nil; nn[$A] = nil
  local nlog = 0
  for r = 1, n do
    if r == n // 4 then
      setmetatable(nn, { __newindex = function(t, k, v) nlog = nlog + 1; rawset(t, k, v) end })
    end
    if r == (3 * n) // 4 then getmetatable(nn).__newindex = nil end
    put$id(nn, "a", r); put$id(nn, "b", r % 3 == 0 and nil or r); put$id(nn, $A, r); put$id(nn, "c", r)
    if r % 7 == 0 then rawset(nn, "a", nil); nn.c = nil end
    for i = 1, #names do
      local k = names[i]
      put$id(c, k, (c[k] or 0) + 1)
    end
    for i = 1, #ik do put$id(h, ik[i], r + i); acc = acc + (h[ik[i]] or 0) end
    put$id(mt, "x", r); put$id(mt, $A, r); put$id(mt, "y" .. (r % 4), r)
    put$id(old, "s", { r }); put$id(old, 1 << 41, "v" .. r); put$id(old, 1, "w" .. r)
    put$id(o, r % 5 + 1, r); put$id(o, -(r % 3), r)
    if r % $M == 0 then
      for i = 1, #names, 2 do put$id(c, names[i], nil) end
      for i = 1, #ik, 3 do put$id(h, ik[i], nil) end
      rawset(mt, "x", nil); put$id(o, r % 5 + 1, nil)
    end
    if r == n // 3 then getmetatable(mt).__newindex = nil end
    if r == n // 2 then getmetatable(mt).__newindex = function(t, k, v) nic = nic + 100; rawset(t, k, v) end end
    if r % 17 == 0 then collectgarbage("step", 0) end
  end
  collectgarbage()
  acc = acc + nlog + (nn.a or 0) + (nn.b or 0)
  return acc + nic + #old.s + #old[1 << 41], srt$id(c) .. srt$id(h) .. srt$id(mt) .. srt$id(o)
end
local function all$id()
  local a, s = run$id($N)
  mix(a); mix(#s)
  mix(select(2, pcall(function() local t = { x = 1 }; for i = 1, 20 do if i == 15 then put$id(t, nil, 1) end; put$id(t, "x", i) end end)))
  mix(select(2, pcall(function() local t = { [5] = 1 }; for i = 1, 20 do if i == 15 then put$id(t, 0 / 0, 1) end; put$id(t, 5, i) end end)))
end
run("bb2$id", all$id)
]==], { id = id, A = rnd(1, 900), B = rnd(2000, 9000), M = rnd(5, 13), N = N(100, 500) })
end
local BB2 = #TT
-- G4 / G5: `string.format("%s", s)` folded to `s` (a short string; numbers, long strings, tables with `__tostring`, extra arguments, other formats and a
-- replaced `string.format` must still behave) and `==` / `~=` of long strings (same object, other length, difference at the head / middle / tail,
-- equal content in two objects, shared last eight bytes, long next to short)
TT[#TT + 1] = function(id)
  return tpl([==[
local fmt$id = string.format
local function run$id(n)
  local P = string.rep("pq", $L)
  local S, A, B, C, D = {}, {}, {}, {}, {}
  for k = 1, 24 do
    S[k] = "s$A-" .. k
    A[k] = P .. "num " .. k
    B[k] = P .. "num " .. (k % $M == 0 and k or k + 100)
    C[k] = P:sub(1, $L) .. "#" .. P:sub($L + 2) .. "num " .. k
    D[k] = (P .. "num "):rep(1) .. k
  end
  local mt = setmetatable({}, { __tostring = function() return "TS$A" end })
  local mixed = { "a", 5, 2.5, true, mt, P .. "z", "b", -3 }
  local acc, eq = 0, 0
  for r = 1, n do
    for k = 1, #S do
      local x = fmt$id("%s", S[k])
      acc = (acc * 31 + #x + (x == S[k] and 1 or 0)) % 1000003
      if A[k] == B[k] then eq = eq + 1 end
      if A[k] ~= C[k] then eq = eq + 2 end
      if A[k] == D[k] then eq = eq + 3 end
      if A[k] == A[(k % 24) + 1] then eq = eq + 7 end
      if A[k] == S[k] then eq = eq + 11 end
    end
    local m = mixed[r % #mixed + 1]
    acc = (acc + #fmt$id("%s", m)) % 1000003
    acc = (acc + #fmt$id("%s", "x", m)) % 1000003
    acc = (acc + #fmt$id(r % 5 == 0 and "%s!" or "%s", "w")) % 1000003
    if r == n // 3 then A[3] = B[3] end
    if r == n // 2 then C[5] = A[5]; D[7] = P end
    if r == (2 * n) // 3 then fmt$id = function(f, s) return "<" .. tostring(s) .. ">" end end
  end
  fmt$id = string.format
  return acc, eq
end
local function all$id()
  local a, e = run$id($N)
  mix(a); mix(e)
  mix(select(2, pcall(function() for i = 1, 30 do fmt$id("%s", (i == 25) and nil or "k") end end)))
  mix(select(2, pcall(function() for i = 1, 30 do local _ = ((i == 25) and {} or "k" .. string.rep("x", 50)) == "k" end end)))
end
run("g45$id", all$id)
]==], { id = id, A = rnd(1, 900), L = rnd(22, 30), M = rnd(3, 9), N = N(60, 300) })
end
-- G1 (`tset` bit 3): GETTABLE with an integer key outside the array part probes the hash part inline: key 0 / negative / huge keys in the hash part
-- of tables with a small array (towers' piles[0]), nil-valued nodes with and without `__index`, float keys with integer-like bit patterns, keys that
-- migrate between the parts, shapes changing in the middle of the loop
TT[#TT + 1] = function(id)
  return tpl([==[
local function get$id(t, k) return t[k] end
local function run$id(n)
  local piles = { { 1 }, { 2 }, [0] = { 0 } }
  local h = { [0] = 1, [-5] = 2, [1 << 33] = 3, [math.maxinteger] = 4, [$A] = 5, [1] = 6, [$B] = 7 }
  local base = { [0] = "b0", [$A] = "ba" }
  local m = setmetatable({ [0] = "m0", [$B] = "mb" }, { __index = function(t, k) return "f" .. tostring(k) end })
  local m2 = setmetatable({ [0] = "x0", [-5] = "x5" }, { __index = base })
  local fl = { [1] = "one", [2] = "two", [0] = "zero" }
  local ks = { 0, -5, 1 << 33, math.maxinteger, $A, 1, 2, 3, $B, 5e-324, 1e-323, 0.0, 2.0, 1.5, "s", -1 }
  local acc = 0
  for r = 1, n do
    for i = 1, #ks do
      local k = ks[i]
      local a, b, c, d, e = get$id(h, k), get$id(m, k), get$id(m2, k), get$id(fl, k), get$id(piles, k)
      acc = (acc * 31 + (type(a) == "number" and a or 1) + #tostring(b) + #tostring(c) + #tostring(d) + (e and 1 or 0)) % 1000003
    end
    piles[0] = { r }
    if r % $M == 0 then h[0] = nil; m[0] = nil; m2[0] = nil; h[$A] = nil; fl[1] = nil end
    if r % $M == 3 then h[0] = r; m[0] = "m" .. r; m2[0] = "x" .. r; h[$A] = r; fl[1] = "o" .. r; for i = 1, 6 do h[i] = i end end
    if r == n // 2 then for i = 2, 6 do h[i] = nil end; h[$B + 1] = 1 end
    if r % 17 == 0 then collectgarbage("step", 0) end
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  mix(select(2, pcall(function() local t = nil; for i = 1, 20 do if i == 15 then t = 5 end; get$id(t, 0) end end)))
  mix(select(2, pcall(function() local t = {}; for i = 1, 20 do if i == 15 then t = "str" end; mix(get$id(t, 0) == nil) end end)))
end
run("g1$id", all$id)
]==], { id = id, A = rnd(2, 900), B = rnd(2000, 9000), M = rnd(5, 13), N = N(80, 400) })
end

-- G2: the C-stack guard of an inlined `pcall` is checked once per loop entry (hoisted out of the body): loops with `pcall` / `xpcall` of Lua
-- closures (error values, several results, a failing iteration in the middle) entered from shallow and from deep `pcall` recursion, so that
-- the entry check passes or fails with the C depth (C stack overflow raised by the interpreter's pcall)
TT[#TT + 1] = function(id)
  return tpl([==[
local function ok$id(i) return i + $A end
local function mf$id(i, k) if i == k then error("e" .. i) end return i * 2, i end
local function loop$id(n, k)
  local s = 0
  for i = 1, n do
    local _, v = pcall(ok$id, i); s = s + v
    local o2, v2, w2 = pcall(mf$id, i, k); s = s + (o2 and v2 + w2 or #v2)
    local _, v3 = xpcall(ok$id, print, i); s = s + v3
  end
  return s
end
local function deep$id(d, n, k)
  if d == 0 then return loop$id(n, k) end
  local ok, v = pcall(deep$id, d - 1, n, k)
  if not ok then error(v, 0) end
  return v
end
local function all$id()
  for _, d in ipairs{0, 3, $D1, 180, $D2, 195, $D3, 199, 205} do
    local ok, v = pcall(deep$id, d, $N, $K)
    mix(ok); mix(ok and v or #tostring(v):gsub("^.-:%d+: ", ""))
  end
end
run("g2p$id", all$id)
]==], { id = id, A = rnd(1, 99), D1 = rnd(20, 150), D2 = rnd(181, 192), D3 = rnd(193, 198), N = N(60, 250), K = rnd(10, 40) })
end
-- H1: integer keys in the hash part read, written and removed on traces (key 0, negative, huge, float with an integer value, the
-- 2^53 boundary), keys that migrate between the array and the hash part, growth of the node vector in the middle of a hot loop, nil-valued
-- nodes with and without `__index` / `__newindex`
TT[#TT + 1] = function(id)
  return tpl([==[
local function rd$id(t, k) return t[k] end
local function wr$id(t, k, v) t[k] = v end
local function run$id(n)
  local h = { [0] = 1, [-$A] = 2, [1 << 40] = 3, [math.maxinteger] = 4, [math.mininteger] = 5, [$B] = 6, [1] = 7 }
  local mi = setmetatable({ [0] = 8, [-3] = 9 }, { __index = function(t, k) return k * 2 end })
  local mn = setmetatable({ [0] = 10, [$B] = 11 }, { __newindex = function(t, k, v) rawset(t, k, v and v + 1) end })
  local ks = { 0, -$A, 1 << 40, math.maxinteger, math.mininteger, $B, 1, 2, 3.0, -3, 9007199254740992 }
  local acc = 0
  for r = 1, n do
    for i = 1, #ks do
      local k = ks[i]
      local a, b = rd$id(h, k), rd$id(mi, k)
      acc = (acc * 33 + (type(a) == "number" and a or 1) + (b or 0)) % 1000003
      wr$id(h, k, (a or 0) + 1)
      wr$id(mn, k, r)
    end
    if r % $M == 0 then h[0] = nil; h[$B] = nil; mi[0] = nil; mn[0] = nil; h[1 << 40] = nil end
    if r % $M == 2 then for i = 1, 40 do h[i] = i end end
    if r % $M == 4 then for i = 1, 40 do h[i] = nil end; h[-$A] = nil end
    if r == n // 2 then for i = 1, 70 do h[-i - 100] = i end end
    if r % 13 == 0 then collectgarbage("step", 0) end
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  mix(select(2, pcall(function() local t = {}; for i = 1, 20 do t[i - 10] = i; if i == 15 then t = 5 end end end)))
end
run("h1k$id", all$id)
]==], { id = id, A = rnd(2, 900), B = rnd(2000, 9000), M = rnd(5, 13), N = N(80, 400) })
end

-- H1: towers-shaped recursion through methods with piles keyed 0..2 or 1..3 (key 0 in the hash part, nil vs table values): exits inside inlined
-- frames of function-entry / continuation traces get side traces with virtual frames
TT[#TT + 1] = function(id)
  return tpl([==[
local D$id = {}
D$id.__index = D$id
local function nd$id(size) return setmetatable({ size = size, nxt = nil }, D$id) end
local T$id = {}
T$id.__index = T$id
function T$id:push(disk, pile)
  local top = self.piles[pile]
  if top ~= nil and disk.size >= top.size then error("big") end
  disk.nxt = top
  self.piles[pile] = disk
end
function T$id:pop(pile)
  local top = self.piles[pile]
  if top == nil then error("empty") end
  self.piles[pile] = top.nxt
  top.nxt = nil
  return top
end
function T$id:move(from, to)
  self:push(self:pop(from), to)
  self.moves = self.moves + 1
end
function T$id:go(disks, from, to)
  if disks == 1 then
    self:move(from, to)
  else
    local other = 3 - from - to
    self:go(disks - 1, from, other)
    self:move(from, to)
    self:go(disks - 1, other, to)
  end
end
local function run$id(n, base, disks)
  local total = 0
  for it = 1, n do
    local t = setmetatable({ piles = {}, moves = 0 }, T$id)
    for i = disks, 0, -1 do t:push(nd$id(i), base + 1) end
    local ok = pcall(t.go, t, disks, base + 1, base + 2)
    total = total + t.moves + (ok and 0 or 1)
    if it % 3 == 0 then t.piles[base] = nd$id($A) end
  end
  return total
end
local function all$id()
  mix(run$id($N, 0, $D))
  mix(run$id($N // 2, -1, $D - 1))
  mix(run$id($N // 3, 5, $D))
end
run("h1t$id", all$id)
]==], { id = id, A = rnd(1, 9), D = rnd(3, 6), N = N(10, 40) })
end
-- H2: exits inside inlined frames (a type change in the leaf) that the inline stub of the exit restores itself: the rebuilt frames are shown to
-- debug.getinfo / debug.traceback, to calls that want 1 / 2 / all results, to tail calls and to error unwinding
TT[#TT + 1] = function(id)
  return tpl([==[
local function leaf$id(x, mode)
  local y = x + $A
  if mode == 1 then
    local a, b = debug.getinfo(2, "l"), debug.getinfo(3, "l")
    return a.currentline, b and b.currentline or -1
  elseif mode == 2 then
    local tb = debug.traceback("t", 1)
    return #tb, select(2, tb:gsub("\n", "\n"))
  elseif mode == 3 then
    return select("#", y, y, y), y
  elseif mode == 4 and x % 7 == 0 then
    error("boom" .. x)
  end
  return y, y * 2
end
local function mid$id(x, mode)
  local a, b = leaf$id(x, mode)
  return a, b
end
local function tl$id(x, mode)
  return leaf$id(x, mode)
end
local function wrap$id(x, mode)
  local r1 = mid$id(x, mode)
  local r2, r3 = tl$id(x, mode)
  local r4 = (tl$id(x, mode))
  return r1, r2, r3, r4
end
local function run$id(n, flip)
  local R = {}
  for i = 1, n do
    local x = i
    if i % $M == 0 then x = i + 0.5 end
    local mode = 0
    if i > flip then mode = i % 5 end
    local ok, r1, r2, r3, r4 = pcall(wrap$id, x, mode)
    R[#R + 1] = (ok and 1 or 0) + (tonumber(r1) or 0) * 3 + (tonumber(r2) or 0) * 5 + (tonumber(r3) or 0) * 7 + (tonumber(r4) or 0) * 11
  end
  local s = 0
  for i = 1, #R do s = (s * 31 + math.floor(R[i])) % 1000000007 end
  return s
end
local function all$id()
  mix(run$id($N, $F))
  mix(run$id($N // 2, 10))
end
run("h2f$id", all$id)
]==], { id = id, A = rnd(1, 9), M = rnd(5, 70), F = rnd(40, 200), N = N(300, 700) })
end
-- BUNDLE-J1 (1): fused HREFK+HLOAD tag checks with two exits (the type guard has its own snapshot): objects of one shape whose fields change
-- type late (integer, float, string, boolean, nil, table), a key that moves out of its node, a missing key, the same reads behind a call
TT[#TT + 1] = function(id)
  return tpl([==[
local function mk$id(i)
  return { a = i, b = i * 0.5, c = "s" .. (i % 3), d = (i % 2 == 0), e = $A }
end
local function rd$id(o, k)
  local s = o.a + o.b + o.e
  if o.d then s = s + #o.c end
  if k % $M == 0 then s = s - o.a end
  return s
end
local function run$id(n)
  local objs = { mk$id(1), mk$id(2), mk$id(3), mk$id(4) }
  local acc = 0
  for i = 1, n do
    local o = objs[i % 4 + 1]
    acc = acc + rd$id(o, i)
    if i == n // 5 then objs[2].a = 2.5 end
    if i == n // 3 then objs[3].b = 7 end
    if i == n // 2 then objs[1].d = nil end
    if i == (2 * n) // 3 then objs[4].e = 1.5 end
    if i == (3 * n) // 4 then for q = 1, 12 do objs[1]["x" .. q] = q end end
    if i == (4 * n) // 5 then objs[2].c = 5 end
  end
  return acc
end
local function all$id()
  mix(run$id($N))
  mix(select(2, pcall(function() local o = mk$id(1); o.b = {}; return rd$id(o, 1) end)))
  mix(select(2, pcall(function() local o = mk$id(1); o.a = nil; return rd$id(o, 2) end)))
end
run("j1h$id", all$id)
]==], { id = id, A = rnd(1, 9), M = rnd(3, 11), N = N(150, 700) })
end

-- BUNDLE-J1 (2): sunk tables that are live while a guard fails inside an inlined callee (the frameful inline materialising exit stub) or in
-- the loop body (the frameless one): integer, float, string and table fields (registers, xmm registers, spill slots), a metatable, aliases,
-- an error raised in the callee, a coroutine that yields the table
TT[#TT + 1] = function(id)
  return tpl([==[
local P$id = {}
P$id.__index = P$id
local function pt$id(x, y) return setmetatable({ x = x, y = y, tag = "t" .. (x % 3) }, P$id) end
local function norm$id(v, k)
  if k % $M == 0 then return v.x * 2 end
  return v.x + v.y
end
local function name$id(t, k)
  if k % $K == 0 then return t.tag .. "!" end
  return t.tag
end
local function boom$id(t, k)
  if k == $B then error("boom " .. t.x) end
  return t.x
end
local keep$id = {}
local function run$id(n)
  local acc, len = 0, 0
  for i = 1, n do
    local v = pt$id(i, i * 0.5)
    local w = { p = v, q = i + 0.25, s = "z" }
    acc = acc + norm$id(v, i) + norm$id(w.p, i + 1) + w.q
    len = len + #name$id(v, i)
    if i % $L == 0 then keep$id[#keep$id % 4 + 1] = v end
    if i == n // 2 then v.y = 5 end
    if i == (2 * n) // 3 then P$id.extra = 1 end
  end
  return acc, len
end
local function all$id()
  local a, b = run$id($N)
  mix(a); mix(b)
  mix(select(2, pcall(function() local t = pt$id(3, 4); local s = 0; for i = 1, $B + 20 do s = s + boom$id(t, i) end; return s end)))
  local co = coroutine.wrap(function(n)
    local s = 0
    for i = 1, n do
      local v = pt$id(i, 0.5)
      s = s + norm$id(v, i)
      if i % 60 == 0 then coroutine.yield(v) end
    end
    return s
  end)
  local r = co($N)
  mix(type(r) == "table" and r.x or r)
  r = co()
  mix(type(r) == "table" and r.x or r)
  mix(#keep$id)
end
run("j1s$id", all$id)
]==], { id = id, M = rnd(3, 11), K = rnd(3, 9), L = rnd(7, 30), B = rnd(60, 200), N = N(150, 600) })
end

-- BUNDLE-J2 (1): the recursion gate: method-call chains with no recursion (their continuation heads are refused and dropped) next to a
-- recursion with data-dependent paths (a method, a local function, a tail call), an error raised at depth, a coroutine yielding inside the
-- recursion, and a callee that changes under the call sites
TT[#TT + 1] = function(id)
  return tpl([==[
local O$id = {}
O$id.__index = O$id
function O$id:a(k) self.n = self.n + k; return self.n end
function O$id:b(k) return self:a(k) + self:a(k + $A) end
function O$id:c(k) local x = self:b(k); return x + self:a(x % 5) + self:b(2) end
local function chain$id(o, n)
  local s = 0
  for i = 1, n do s = s + o:c(i % $M) end
  return s
end
local function rec$id(t, d)
  if d <= 0 then return 1 end
  local s = 0
  for i = 1, #t do
    local v = t[i]
    if v % 3 == 0 then s = s + rec$id(t, d - 1)
    elseif v % 3 == 1 then s = s + 2
    else s = s + (v > $T and 3 or 4) end
  end
  return s
end
local function deep$id(n) if n == 0 then error("deep" .. n) end return deep$id(n - 1) + 1 end
local function tailr$id(n, a) if n == 0 then return a end return tailr$id(n - 1, a + n) end
local N$id = {}
function N$id:sum(d)
  if d == 0 then return self.v end
  local s = self.v
  if self.l then s = s + self.l:sum(d - 1) end
  if self.r then s = s + self.r:sum(d - 1) end
  return s
end
local function mk$id(d, v)
  local t = setmetatable({ v = v }, { __index = N$id })
  if d > 0 then t.l = mk$id(d - 1, v * 2); if d % 2 == 0 then t.r = mk$id(d - 1, v * 2 + 1) end end
  return t
end
local function all$id()
  local o = setmetatable({ n = 0 }, O$id)
  mix(chain$id(o, $N))
  local t = {}
  for i = 1, $L do t[i] = (i * $A) % 29 end
  local acc = 0
  for r = 1, $R do
    acc = acc + rec$id(t, 1 + r % 3)
    t[r % $L + 1] = (t[r % $L + 1] + r) % 31
  end
  mix(acc)
  mix(select(2, pcall(deep$id, $D)))
  mix(tailr$id($D * 2, 0))
  local root = mk$id(5, 1)
  local s = 0
  for r = 1, $R do s = s + root:sum(r % 6) end
  mix(s)
  local co = coroutine.wrap(function(k)
    local function y(d) if d == 0 then coroutine.yield(d) return 1 end return y(d - 1) + 1 end
    local z = 0
    for i = 1, k do z = z + y(i % 4) end
    return z
  end)
  mix(co($R)); mix(co())
  O$id.a = function(self, k) return k * 2 end
  mix(chain$id(o, $N // 2))
end
run("j2r$id", all$id)
]==], { id = id, A = rnd(1, 9), M = rnd(3, 11), T = rnd(10, 25), L = rnd(6, 14), N = N(100, 500), R = N(40, 160), D = rnd(8, 60) })
end

-- BUNDLE-J2 (2): per-site deeper inlining: recursion in a hot loop (fib / ackermann shapes whose argument varies per iteration, a recursion
-- deeper than any limit, a wide body), an error and a yield inside the recursion, a callee replaced in the middle of the loop
TT[#TT + 1] = function(id)
  return tpl([==[
local function fib$id(n) if n < 2 then return n end return fib$id(n - 1) + fib$id(n - 2) end
local function ack$id(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack$id(m - 1, 1) end
  return ack$id(m - 1, ack$id(m, n - 1))
end
local function wide$id(n, a, b)
  if n < 2 then return a + b end
  local x = wide$id(n - 1, a + 1, b * 2 % 1009)
  local y = wide$id(n - 2, b, a)
  local z = (x * 3 + y * 5 + a * 7 + b * 11) % 100003
  return (z * z + x + y + n) % 100003
end
local function run$id(n)
  local s = 0
  for i = 1, n do s = s + fib$id(i % 3 + $A) + ack$id(1, i % 4) end
  return s
end
local function deeper$id(n)
  local s = 0
  for i = 1, n do s = s + fib$id(9 + i % 4) end
  return s
end
local function wl$id(n)
  local s = 0
  for i = 1, n do s = (s + wide$id(i % 4 + 3, i, i + 1)) % 1000003 end
  return s
end
local function bad$id(n) if n == 0 then error("bad") end return bad$id(n - 1) + 1 end
local function all$id()
  local t = 0
  for r = 1, $R do t = t + run$id($N) end
  mix(t)
  mix(deeper$id($N))
  mix(wl$id($N))
  local co = coroutine.wrap(function(n)
    local s = 0
    for i = 1, n do
      s = s + fib$id(i % 4 + 3)
      if i % 7 == 0 then coroutine.yield(s) end
    end
    return s
  end)
  local r = co($N)
  mix(r)
  while true do
    r = co()
    if r == nil then break end
    mix(r)
    if r > 100000000 then break end
  end
  for r = 1, 3 do mix(select(2, pcall(function() local s = 0; for i = 1, $N do s = s + fib$id(i % 5 + 2); if i == $B then s = s + bad$id(3) end end; return s end))) end
  local old = fib$id
  fib$id = function(n) return old(n) + 1 end
  mix(run$id($N // 2))
end
run("j2d$id", all$id)
]==], { id = id, A = rnd(3, 5), N = N(60, 200), R = N(10, 40), B = rnd(20, 50) })
end

-- BUNDLE-K (B): method calls on strings (SELF on a string receiver in tier 1): plain calls, a polymorphic site (strings and tables), long
-- strings, monkey-patching of string.* in the middle of the loop, a replaced / removed / table / function __index of the string metatable,
-- an __index chain, a nil-valued method, no string metatable at all
TT[#TT + 1] = function(id)
  return tpl([==[
local obj$id = { sub = function(self, a, b) return "obj" .. a end, upper = function() return "OBJ" end, find = function() return 7 end, len = function() return 3 end, byte = function() return 5 end }
local function meth$id(s, n)
  local acc = 0
  for i = 1, n do
    local r = s:sub(i % 5 + 1, i % 7 + 3)
    acc = acc + #tostring(r) + (s:find("a", 1, true) or 0) + #tostring(s:upper()) + s:len() + (s:byte(1 + i % 3) or 0)
  end
  return acc
end
local function poly$id(n)
  local items = { "abcabc", obj$id, ("xyz"):rep(30), "q" }
  local acc = 0
  for i = 1, n do
    local s = items[i % 4 + 1]
    acc = acc + #tostring(s:sub(1, 2)) + #tostring(s:upper())
  end
  return acc
end
local function patched$id(n, at, patch, unpatch)
  local acc = 0
  for i = 1, n do
    if i == at then patch() end
    local ok, r = pcall(function() local s = "hello world"; return #tostring(s:sub(1, i % 5 + 1)) + #tostring(s:upper()) + s:len() end)
    acc = acc + (ok and r or #r)
  end
  unpatch()
  return acc
end
local function all$id()
  local S = string
  local mt = getmetatable("")
  local oidx = mt.__index
  mix(meth$id("banana bandana", $N))
  mix(meth$id(("ab"):rep(40), $N))
  mix(poly$id($N))
  -- monkey-patch string.sub / string.len mid-loop, then restore; remove a method (error), set it to nil, restore
  local osub, olen, oup = S.sub, S.len, S.upper
  mix(patched$id($N, $K, function() S.sub = function(s, i, j) return "P" .. i end end, function() S.sub = osub end))
  mix(patched$id($N, $K, function() S.len = function() return 42 end end, function() S.len = olen end))
  mix(patched$id($N, $K, function() S.upper = nil end, function() S.upper = oup end))
  mix(patched$id($N, $K, function() S.upper = function(s) return s .. "!" end end, function() S.upper = oup end))
  mix(patched$id($N, $K, function() rawset(S, "extra", 1); S.upper = nil; S.upper = oup end, function() S.upper = oup end))
  -- replace __index by another table, by a chain, by a function; remove it; drop the string metatable
  local alt = { sub = function(s, i, j) return "ALT" end, upper = function() return "ALTUP" end, len = function() return 1 end }
  mix(patched$id($N, $K, function() mt.__index = alt end, function() mt.__index = oidx end))
  mix(patched$id($N, $K, function() mt.__index = setmetatable({}, { __index = S }) end, function() mt.__index = oidx end))
  mix(patched$id($N, $K, function() mt.__index = setmetatable({ sub = alt.sub }, { __index = function(t, k) return S[k] end }) end, function() mt.__index = oidx end))
  mix(patched$id($N, $K, function() mt.__index = function(s, k) return alt[k] or S[k] end end, function() mt.__index = oidx end))
  mix(patched$id($N, $K, function() mt.__index = nil end, function() mt.__index = oidx end))
  mix(patched$id($N, $K, function() mt.__index = 5 end, function() mt.__index = oidx end))
  mix(patched$id($N, $K, function() debug.setmetatable("", nil) end, function() debug.setmetatable("", mt) end))
  mix(patched$id($N, $K, function() debug.setmetatable("", { __index = alt }) end, function() debug.setmetatable("", mt) end))
  -- the original __index table gets a metatable of its own / a replaced metatable of the metatable
  mix(patched$id($N, $K, function() setmetatable(S, { __index = function(t, k) return alt[k] end }); S.sub = nil end, function() setmetatable(S, nil); S.sub = osub end))
  mix(meth$id("banana bandana", $N))
  mix(select(2, pcall(function() return ("x"):nosuch() end)))
  mix(select(2, pcall(function() local n = 5; return n:sub(1) end)))
end
run("k1s$id", all$id)
]==], { id = id, N = N(40, 160), K = rnd(3, 30) })
end

-- BUNDLE-K (A): pattern calls (captures not zeroed any more): captures, position captures, back references, %f, %b, anchors, errors,
-- 32 and 33 captures, gsub with string / table / function replacements and %0..%9, gmatch with captures, find with init
TT[#TT + 1] = function(id)
  return tpl([==[
local subj$id = { "hello world from lua", "key = value; other = thing", "(a(b)c) [x] {y}", "THE (quick) fox", "aXbXcXabcabc", "", "  trim me  ", "2024-11-06T12:30:45Z", "abab abab" }
local pats$id = { "(%w+)", "()%w+()", "(%w+)%s*=%s*(%w+)", "%f[%w]%w+", "%b()", "(a)(b)(c)", "(.)%1", "^(%s*)(.-)(%s*)$", "(%d+)-(%d+)-(%d+)", "((a)(b))", "%((%w+)%)",
  "(", ")", "%", "(%1)", "[a", "%b", "%f", "(()", "((((((((((((((((((((((((((((((((a))))))))))))))))))))))))))))))))", "((((((((((((((((((((((((((((((((.))))))))))))))))))))))))))))))))",
  "%1", "(a)%2", "(.-)X", "(%a)(%a)?", "()", "^()", "$()" }
local function show$id(...)
  local t = table.pack(...)
  for i = 1, t.n do t[i] = tostring(t[i]) end
  return table.concat(t, "|")
end
local function one$id(s, p, init)
  local o = {}
  o[#o + 1] = show$id(pcall(string.find, s, p, init))
  o[#o + 1] = show$id(pcall(string.match, s, p, init))
  o[#o + 1] = show$id(pcall(string.gsub, s, p, "<%0>"))
  o[#o + 1] = show$id(pcall(string.gsub, s, p, "%1%1"))
  o[#o + 1] = show$id(pcall(string.gsub, s, p, function(a, b) return a and ("[" .. tostring(a) .. tostring(b) .. "]") or nil end, 3))
  o[#o + 1] = show$id(pcall(string.gsub, s, p, { hello = "H", key = false, a = 1 }))
  o[#o + 1] = show$id(pcall(function() local n = 0; local r = {}; for a, b in s:gmatch(p) do n = n + 1; r[#r + 1] = tostring(a) .. tostring(b); if n > 12 then break end end; return n, table.concat(r, ",") end))
  return table.concat(o, ";")
end
local function all$id()
  local h = 0
  for r = 1, $R do
    for si = 1, #subj$id do
      for pi = 1, #pats$id do
        local s = one$id(subj$id[si], pats$id[pi], (r + si + pi) % 3 == 0 and 2 or nil)
        h = (h * 31 + #s) % 1000003
        if r == 1 then mix(s) end
      end
    end
  end
  mix(h)
  -- a method-call site on strings over the same patterns
  local acc = 0
  for i = 1, $R * 20 do
    local s = subj$id[i % #subj$id + 1]
    local a, b = s:match("(%w+)%s*(%w*)")
    acc = acc + #(a or "") + #(b or "") + (s:find("%f[%w]%w", 2) or 0) + select(2, s:gsub("%w+", "%0"))
  end
  mix(acc)
end
run("k2p$id", all$id)
]==], { id = id, R = N(3, 12) })
end

-- BUNDLE-L (D1a): collectables (tables, closures, strings) stored into closed upvalues from hot loops with GC pressure; the value is read
-- back one iteration later after allocations that would reuse its memory if it had been collected; a shared upvalue seen by two closures
TT[#TT + 1] = function(id)
  return tpl([==[
local function mk$id()
  local last, cnt, fn, str, tbl = nil, 0, nil, "s", {}
  local function put(v) last = v; cnt = cnt + 1 end
  local function putf(f) fn = f end
  local function puts(s) str = s end
  local function putt(t) tbl = t end
  local function get() return last, cnt, fn, str, tbl end
  return put, putf, puts, putt, get
end
local function loopU$id(put, putf, puts, putt, get, n)
  local acc = 0
  for i = 1, n do
    local p = get()
    if p then acc = acc + p[1] + p[2][1] end
    local g = { i, { i } }
    put(g)
    if i % $M == 0 then putf(function() return i end) end
    puts("k" .. (i % 7))
    putt({ x = i, y = { z = i } })
    local j1, j2 = { i + 1, { i + 1 } }, { i + 2, { i + 2 } }
    acc = acc + #g + #j1 - #j2
  end
  local l, c, f, s, t = get()
  return l[1] + l[2][1], c, f and f() or -1, s, t.x + t.y.z, acc
end
local function shared$id(n)
  local cell = nil
  local function set(v) cell = v end
  local function read() return cell end
  local s = 0
  for i = 1, n do
    set({ i })
    local c = read()
    s = s + c[1]
    local junk = { i, i + 1, i + 2 }
  end
  return s
end
local function all$id()
  for r = 1, $R do
    local a, b, c, d, e = mk$id()
    mix(loopU$id(a, b, c, d, e, $N + r * 7))
    if r % 2 == 0 then collectgarbage() end
  end
  mix(shared$id($N), shared$id($N + 3))
end
run("l1u$id", all$id)
]==], { id = id, N = N(40, 160), R = N(4, 10), M = rnd(2, 5) })
end

-- BUNDLE-L (D1b): loads of absent keys: integer keys outside the array part (negative, zero, past the end), string keys read from a name list,
-- keys that appear / vanish / move into the array part / rehash between calls, nil-valued nodes, metatables with and without __index added
-- in the middle (function, table, chain), a table that gets __newindex
TT[#TT + 1] = function(id)
  return tpl([==[
local function loadInt$id(t, n, base)
  local c, nils = 0, 0
  for i = 1, n do
    local v = t[i * 3 + base]
    if v == nil then nils = nils + 1 else c = c + v end
  end
  return c, nils
end
local function loadStr$id(t, names, n)
  local c, nils = 0, 0
  local m = #names
  for i = 1, n do
    local v = t[names[i % m + 1]]
    if v == nil then nils = nils + 1 else c = c + (type(v) == "number" and v or 1) end
  end
  return c, nils
end
local function all$id()
  local sparse = {}
  for k = 1, $A do sparse[k * 9] = k end
  sparse[-5] = 7; sparse[0] = 9
  local names = {}
  for i = 1, $A do names[i] = "nm" .. i end
  local reg = {}
  for i = 1, $A, 2 do reg[names[i]] = i end
  local grow = {}
  local arr = { 1, 2, 3 }
  for r = 1, $R do
    mix(loadInt$id(sparse, $N, r % 3), loadInt$id(grow, $N, -2), loadInt$id(arr, $N, 0), loadStr$id(reg, names, $N))
    if r == $K then for k = 1, 30 do sparse[k * 9 + 1] = k end end
    if r == $K + 1 then for i = 1, 24 do grow[i] = i end end
    if r == $K + 2 then for k = 1, $A do sparse[k * 9] = nil end end
    if r == $K + 2 then for i = 2, $A, 2 do reg[names[i]] = -i end end
    if r == $K + 3 then arr[4] = 4; arr[5] = 5; for i = 1, $A, 3 do reg[names[i]] = nil end end
    if r == $K + 4 then setmetatable(sparse, { __index = function(t, k) return 1 end }); setmetatable(reg, { __index = function(t, k) return #k end }) end
    if r == $K + 5 then for i = 1, 100 do reg["x" .. i] = i end; arr[1000] = 1 end
    if r == $K + 6 then setmetatable(sparse, { __index = { [14] = 100, [17] = 50 } }); setmetatable(reg, { __index = { nm2 = 1000 } }) end
    if r == $K + 7 then setmetatable(sparse, {}); setmetatable(reg, { __newindex = function() end }); setmetatable(arr, { __index = function(t, k) return -1 end }) end
    if r == $K + 8 then setmetatable(arr, nil); sparse[2] = 1; sparse[3] = 2 end
  end
end
run("l1a$id", all$id)
]==], { id = id, A = rnd(20, 40), N = N(40, 120), R = rnd(14, 18), K = rnd(3, 5) })
end

-- BUNDLE-L (D1c): keys built by CONCAT (loads, stores of existing keys, absent keys that appear later), an integer-to-string key, a long key,
-- a metatable with __index added in the middle
TT[#TT + 1] = function(id)
  return tpl([==[
local function concatLoad$id(t, n, m)
  local c, nils = 0, 0
  for i = 1, n do
    local v = t["key" .. (i % m)]
    if v == nil then nils = nils + 1 else c = c + v end
  end
  return c, nils
end
local function concatStore$id(t, n, m)
  for i = 1, n do
    local k = "key" .. (i % m)
    if t[k] then t[k] = t[k] + 1 end
  end
end
local function all$id()
  local ck = {}
  local long = {}
  for r = 1, $R do
    mix(concatLoad$id(ck, $N, 17), concatLoad$id(long, $N, 3))
    concatStore$id(ck, $N, 17)
    if r == 2 then for i = 0, 16, 2 do ck["key" .. i] = i end end
    if r == 3 then for i = 1, 16, 2 do ck["key" .. i] = 100 + i end end
    if r == 4 then for i = 0, 16, 4 do ck["key" .. i] = nil end end
    if r == 5 then setmetatable(ck, { __index = function(t, k) return 5 end }) end
    if r == 6 then setmetatable(ck, nil); for i = 1, 100 do ck["z" .. i] = i end end
    if r == 7 then for i = 0, 2 do long["key" .. i] = i + 1 end end
    if r == 8 then long.key1 = nil end
  end
  mix(ck.key2, ck.key3, long.key0)
end
run("l1c$id", all$id)
]==], { id = id, N = N(40, 120), R = rnd(9, 12) })
end

-- BUNDLE-L (D3): sinking with the rate policy: metatabled vectors that escape (materialise) at very different rates per site (every iteration, every
-- 3rd, every 40th, never), in loops that run long enough for the policy to demote a site in the middle (the traces of its family are flushed and
-- re-recorded without sinking it), a callee that keeps the vector in some calls, a plain table shape next to it, a `__gc` that appears late
TT[#TT + 1] = function(id)
  return tpl([==[
local V$id = {}
V$id.__index = V$id
local function vec$id(x, y) return setmetatable({ x = x, y = y }, V$id) end
function V$id:add(o) return vec$id(self.x + o.x, self.y + o.y) end
function V$id:dot(o) return self.x * o.x + self.y * o.y end
local kept$id = {}
local function maybe$id(v, i, m) if i % m == 0 then kept$id[#kept$id + 1] = v end return v.x end
local function run$id(n, m1, m2, m3)
  local a, b = vec$id(1, $A), vec$id(0.5, 2)
  local acc = 0
  for i = 1, n do
    local c = a:add(b)
    local p = { x = c.y, y = c.x }
    local d = vec$id(p.x, p.y):add(a)
    acc = acc + d:dot(c) + maybe$id(c, i, m1) + maybe$id(d, i, m2) + maybe$id(p, i, m3)
    if i == n // 2 then b = vec$id(b.y, 3) end
    if i == (3 * n) // 4 then V$id.__gc = function() end end
  end
  return acc
end
local function all$id()
  for r = 1, $R do
    mix(run$id($N, 1, 3, 40))
    mix(run$id($N, 40, 1000000, 1000000))
    mix(run$id($N, 1000000, 1000000, 1000000))
    mix(#kept$id)
    kept$id = {}
    if r == 2 then V$id.__gc = nil end
  end
  collectgarbage()
end
run("l3s$id", all$id)
]==], { id = id, A = rnd(1, 5), N = N(300, 900), R = rnd(3, 6) })
end
SINKT[#SINKT + 1] = #TT

-- BUNDLE-N: `return Class(...)` (TAILCALL of a table with `__call`, vararg and fixed callees), a tail call into a vararg function, `__index` Lua
-- functions at chain depth > 1 (the same prototype twice on the frame stack), `rawget(t, "str")`; the callee, the chain and the metatable change midway
TT[#TT + 1] = function(id)
  return tpl([==[
local Obj$id = {}
Obj$id.__index = Obj$id
function Obj$id:__call(...) local o = setmetatable({}, self) o:new(...) return o end
function Obj$id:new(x, y) self.x = x or 0 self.y = y or $A end
local Vec$id = setmetatable({}, Obj$id)
Vec$id.__index = Vec$id
Vec$id.new = Obj$id.new
Vec$id.__call = Obj$id.__call
Vec$id.__add = function(a, b) return Vec$id(a.x + b.x, a.y + b.y) end
Vec$id.__unm = function(a) return Vec$id(-a.x, -a.y) end
local function va$id(...) return select('#', ...), ... end
local function tva$id(a, b) return va$id(a, b, $A) end
local function tvaN$id(n, ...) return va$id(n, ...) end
local function fix$id(x) return setmetatable({}, { __call = function(self, a, b) return a + (b or 0), x end }) end
local fx$id = fix$id($A)
local function viaFix$id(a) return fx$id(a, $A) end
local function mk$id(parent)
  local dict = {}
  local st = setmetatable({}, { __index = function(_, k) local r = rawget(dict, k) if r == nil and parent then return parent.st[k] end return r end })
  return { dict = dict, st = st }
end
local A$id, B$id, C$id = mk$id(nil), nil, nil
B$id = mk$id(A$id) C$id = mk$id(B$id)
A$id.dict.p = 1 A$id.dict.q = 2 B$id.dict.q = 20 C$id.dict.r = 300
local leaf$id = setmetatable({}, { __index = setmetatable({}, { __index = C$id.st }) })
local function mixa$id(...) for i = 1, select('#', ...) do mix((select(i, ...))) end end
local function all$id()
  for r = 1, $R do
    local v = Vec$id($A, 1)
    for i = 1, $N do
      local w = -(v + Vec$id(i, i))
      local n, a1, a2, a3 = tva$id(i, w.x)
      local m, b1, b2 = tvaN$id(i, w.y)
      local c, d = viaFix$id(i)
      mixa$id(w.x + w.y, n + a1 + a2 + a3, m + b1 + b2, c + d, leaf$id.p, leaf$id.q, leaf$id.r, leaf$id.zz)
      if i == $N // 2 then
        if r == 1 then A$id.dict.zz = "zz" end
        if r == 2 then rawset(leaf$id, "q", 7) end
        if r == 3 then Vec$id.__call = function(self, x, y) return setmetatable({ x = x or 0, y = (y or 0) + 1 }, self) end end
        if r == 4 then getmetatable(fx$id).__call = nil end
        if r == 5 then B$id.dict.p = "bp" end
        if r == 6 then setmetatable(getmetatable(leaf$id).__index, { __index = function(_, k) return k end }) end
      end
    end
    if r == 4 then mixa$id(pcall(viaFix$id, 1)) getmetatable(fx$id).__call = function(self, a, b) return a * 2, b end end
    mixa$id(pcall(function() return rawget(nil, "k") end))
  end
end
run("n1c$id", all$id)
]==], { id = id, A = rnd(1, 9), N = N(40, 160), R = rnd(4, 7) })
end

-- BUNDLE-O: a polymorphic method call site (2..5 receiver classes with different prototypes, a `__index` chain of varying depth, closures of one
-- prototype with different upvalues); the receiver mix, a class method and a chain link change midway
TT[#TT + 1] = function(id)
  return tpl([==[
local P$id = {}
local function m1$id(self, x) self.v = self.v + x return self.v end
local function m2$id(self, x) self.v = self.v - x * 2 return self.v end
local function m3$id(self, x) local t = self.v * 3 + x self.v = t % 1000 return t end
local function m4$id(self, x) self.w = (self.w or 0) + 1 self.v = self.v + self.w return x end
local function m5$id(self, x) if x % 2 == 0 then return self.v end self.v = self.v + 1 return -x end
local function mkc$id(d, r) return function(self, x) self.v = self.v + d * x - r return self.v end end
local base$id = { f = m1$id } base$id.__index = base$id
local kid$id = setmetatable({}, base$id) kid$id.__index = kid$id
local cls$id = { base$id, kid$id, { f = m3$id }, { f = m4$id }, { f = m5$id }, { f = mkc$id(2, 1) }, { f = mkc$id(-3, 4) } }
for i = 3, #cls$id do cls$id[i].__index = cls$id[i] end
setmetatable(cls$id[3], kid$id)
local objs$id = {}
for i = 1, #cls$id do objs$id[i] = setmetatable({ v = i + $A }, cls$id[i]) end
local seed$id = $A * 7919
local function pick$id(k) seed$id = (seed$id * 16807) % 2147483647 return seed$id % k + 1 end
local function all$id()
  for r = 1, $R do
    local k = 2 + (r + $A) % (#objs$id - 1)
    for i = 1, $N do
      local o = objs$id[pick$id(k)]
      mix(o:f(i))
      if i == $N // 2 then
        if r == 1 then cls$id[2].f = m2$id end
        if r == 2 then base$id.f = m5$id end
        if r == 3 then kid$id.f = nil end
        if r == 4 then cls$id[3].f = m1$id end
        if r == 5 then setmetatable(kid$id, nil) end
      end
    end
    mix(pcall(function() return objs$id[2]:f(1) end))
  end
  for i = 1, #objs$id do mix(objs$id[i].v) end
end
run("o1p$id", all$id)
]==], { id = id, A = rnd(1, 9), N = N(60, 240), R = rnd(4, 7) })
end

-- SINK-F: sunk metatabled temporaries live across method calls inside inlined frames of a class-polymorphic call site (the exit inside the frame gets
-- a side trace although its snapshot names a sunk table): a plain frame, a metamethod frame (`__add` / `__unm` with the temporary live), an error raised
-- in the callee, a coroutine yield with the temporary live, an escape through the side trace, the callee and `__gc` change midway
TT[#TT + 1] = function(id)
  return tpl([==[
local Base$id = {}
Base$id.__index = Base$id
local Heavy$id = setmetatable({}, Base$id)
Heavy$id.__index = Heavy$id
local V$id = {}
V$id.__index = V$id
local function vec$id(x, y) local o = setmetatable({}, V$id) o.x = x o.y = y return o end
V$id.__unm = function(a) return vec$id(-a.x, -a.y) end
V$id.__add = function(a, b) return vec$id(a.x + b.x, a.y + b.y) end
local kept$id = {}
function Base$id:push(f) self.fx = self.fx + f.x self.fy = self.fy + f.y end
function Heavy$id:push(f)
  self.fx = self.fx + 2 * f.x
  self.fy = self.fy + f.y
  if self.fx % $K == 0 then kept$id[#kept$id + 1] = f end
end
local objs$id = {}
local function reset$id()
  for i = 1, $NO do objs$id[i] = setmetatable({ fx = 0, fy = 0 }, (i % $H == 0) and Heavy$id or Base$id) end
end
local function apply$id(a, b, k)
  local f = vec$id(k, k * 2)
  a:push(f)
  b:push(-f)
end
local function run$id(n, mode)
  for k = 0, n - 1 do
    local a, b = objs$id[k % $NO + 1], objs$id[(k + 7) % $NO + 1]
    if mode == 1 then
      apply$id(a, b, k % 3)
    elseif mode == 2 then
      local f = vec$id(k % 5, 1)
      local g = f + vec$id(1, k % 3)
      a:push(g)
      b:push(f + g)
    else
      local f = vec$id(k % 4, 3)
      a:push(f)
      if k % $Y == 0 then mix(coroutine.wrap(function(v) coroutine.yield(v.x) return v.y end)(f)) end
    end
    if k == n // 2 then Heavy$id.push = function(self, f) self.fx = self.fx - f.x end end
    if k == (3 * n) // 4 then V$id.__gc = function() end end
  end
end
local function sum$id()
  local t = 0
  for i = 1, $NO do t = t + objs$id[i].fx + objs$id[i].fy end
  return t
end
local function all$id()
  for r = 1, $R do
    reset$id()
    for m = 1, 3 do run$id($N, m) mix(sum$id()) end
    mix(select(2, pcall(function()
      for k = 0, 80 do local f = vec$id(k, 1) objs$id[k % $NO + 1]:push(k == 70 and 5 or f) end
    end)))
    mix(#kept$id)
    kept$id = {}
    if r == 2 then V$id.__gc = nil end
  end
  collectgarbage()
end
run("sf1$id", all$id)
]==], { id = id, K = rnd(5, 19), NO = rnd(12, 40), H = rnd(3, 6), Y = rnd(20, 60), N = N(200, 700), R = rnd(2, 4) })
end
SINKT[#SINKT + 1] = #TT

-- SINK-R: a filter loop over a producer closure (stream pipelines): the record built by the producer is sunk, the predicate's exit leaves the root's
-- pre-roll inside inlined frames with the record live (the ineffective-root rule re-records the root without sinking it); the predicate's kind and
-- the producer change, an error is raised inside the predicate
TT[#TT + 1] = function(id)
  return tpl([==[
local K$id = { "a", "b", "c", "d" }
local seed$id = $S
local function rnd$id(n) seed$id = (seed$id * 16807) % 2147483647 return seed$id % n + 1 end
local function readings$id(n)
  local i = 0
  return function()
    if i >= n then return nil end
    i = i + 1
    return { t = i, kind = K$id[rnd$id(4)], value = (i * 7) % 97 }
  end
end
local function filter$id(pred, src)
  return function()
    while true do
      local v = src()
      if v == nil then return nil end
      if pred(v) then return v end
    end
  end
end
local function count$id(src)
  local n, s = 0, 0
  for v in src do n = n + 1 s = s + v.value end
  return n, s
end
local function all$id()
  for r = 1, $R do
    for _, kind in ipairs(K$id) do
      local n, s = count$id(filter$id(function(x) return x.kind == kind end, readings$id($N)))
      mix(n)
      mix(s)
    end
    if r == 2 then K$id[2] = "b2" end
    mix(select(2, pcall(count$id, filter$id(function(x) if x.t == 55 then error("p " .. x.value) end return x.kind == "a" end, readings$id($N)))))
  end
end
run("sr1$id", all$id)
]==], { id = id, S = rnd(1, 2000000), N = N(1500, 3500), R = rnd(1, 2) })
end
SINKT[#SINKT + 1] = #TT

-- DYNMASK (`dynmask` knob; JITDIFF_TSET=dm draws only these): dynamic short-string keys whose table changes under the trace.
local DMT = {} -- indices of the dynamic-key templates
-- (1) a table that grows / shrinks / gets a metatable while a loop looks up and stores through computed keys: present, absent and nil-valued
-- keys, a rehash between two lookups of one key, a late `__index` / `__newindex`, a table swap, a collection
DMT[#DMT + 1] = #TT + 1
TT[#TT + 1] = function(id)
  return tpl([==[
local N$id = {}
for i = 1, $NK do N$id[i] = "d$id" .. "_" .. i end
local function f$id(t, n, seed, m)
  local s, c = 0, seed
  for i = 1, n do
    c = (c * 1103 + 12345) % 65521
    local k = N$id[c % m + 1]
    local v = t[k]
    if v then s = s + v; t[k] = v + $INC else t[k] = i; s = s + 1 end
    if i % $EV == 0 then
      local k2 = N$id[(c + 7) % m + 1]
      $MUT
    end
    s = s + (t[N$id[(c + i) % m + 1]] or 0)
  end
  return s
end
local function r$id()
  local t = {}
  for r = 1, $R do
    mix(f$id(t, $N, r * 31, $M1))
    mix(f$id(t, $N, r * 17, $M2))
    if r == 2 then
      $MUT2
    end
    if r == 3 then t = {} end
  end
  local cnt = 0
  for _ in pairs(t) do cnt = cnt + 1 end
  mix(cnt)
end
run("dm1$id", r$id)
]==], { id = id, NK = rnd(1, 330), INC = pick{ "1", "2", "0.5", "-1" }, EV = rnd(3, 40), N = N(80, 500), R = rnd(3, 6),
    M1 = rnd(1, 330), M2 = rnd(1, 330),
    MUT = tpl(pick{ "t[k2] = nil", "t[k2] = i", "if i % 2 == 0 then t[k2] = nil else t[k2] = 1 end", "for j = 1, 40 do t[N$id[(c + j) % m + 1]] = j end",
      "local _ = collectgarbage and (i % 5 == 0) and collectgarbage('step')", "rawset(t, k2, i)", "t[k2 .. ''] = i" }, { id = id }),
    MUT2 = tpl(pick{ "setmetatable(t, { __index = function(_, k) return #k end })", "setmetatable(t, { __newindex = function(tt, k, v) rawset(tt, k, v) end })",
      "setmetatable(t, { __index = { [N$id[1] ] = 5, [N$id[2] ] = 6 } })", "for j = 1, 200 do t['x' .. j] = j end", "collectgarbage()",
      "setmetatable(t, { __newindex = {} })", "for kk in pairs(t) do t[kk] = nil end" }, { id = id }) })
end

-- (2) an environment chain: a local miss, then the parents (depth 1-4), computed keys, parents that change or get a function `__index`
DMT[#DMT + 1] = #TT + 1
TT[#TT + 1] = function(id)
  return tpl([==[
local N$id = {}
for i = 1, $NK do N$id[i] = "e$id" .. "_" .. i end
local function mk$id(depth)
  local top = {}
  for i = 1, $NK, 2 do top[N$id[i]] = i end
  local cur = top
  for d = 1, depth do
    local nxt = {}
    for i = d, $NK, 3 do nxt[N$id[i]] = i * 10 + d end
    setmetatable(cur, { __index = nxt })
    cur = nxt
  end
  return top, cur
end
local function f$id(env, n, m)
  local s, nils = 0, 0
  for i = 1, n do
    local v = env[N$id[i % m + 1]]
    if v == nil then nils = nils + 1 else s = s + v end
    if i == $K1 then $MUT end
  end
  return s, nils
end
local function r$id()
  for depth = 0, 3 do
    local top, last = mk$id(depth)
    for r = 1, $R do
      local a, b = f$id(top, $N, $M)
      mix(a); mix(b)
      if r == 2 then
        local k = N$id[$KS]
        top[k] = $KV
        if depth > 0 then getmetatable(top).__index[N$id[$KS + 1] ] = 5 end
      end
      if r == 3 and depth > 0 then getmetatable(top).__index = $NI end
    end
  end
end
run("dm2$id", r$id)
]==], { id = id, NK = rnd(8, 60), N = N(60, 300), R = rnd(3, 5), M = rnd(1, 40), K1 = rnd(5, 50), KS = rnd(1, 6),
    KV = pick{ "nil", "7", "'str'", "false" },
    NI = pick{ "function(t, k) return 3 end", "{}", "nil", "5" },
    MUT = tpl(pick{ "env[N$id[1] ] = nil", "env[N$id[2] ] = 9", "for j = 1, 100 do env['y' .. j] = j end", "setmetatable(env, nil)" }, { id = id }) })
end

-- (3) key kinds in one computed-key loop: short strings, long strings, integers, integer-valued floats, other floats, booleans, nil / NaN
-- (errors for a store, nil for a load), strings that look like numbers
DMT[#DMT + 1] = #TT + 1
TT[#TT + 1] = function(id)
  return tpl([==[
local K$id = { "k1", "k2", "k3", "k4", string.rep("L", 45) .. "a", string.rep("L", 45) .. "b", 1, 2.0, "1", 2.5, true, $EX }
local function f$id(t, n, kinds)
  local s = 0
  for i = 1, n do
    local k = K$id[i % kinds + 1]
    local ok, e = pcall(function() t[k] = (t[k] or 0) + i end)
    if not ok then err(e) else s = s + t[k] end
    s = s + (rawget(t, k) or 0)
  end
  return s
end
local function r$id()
  for r = 1, $R do
    local t = { 10, 20, 30, k1 = 1 }
    mix(f$id(t, $N, #K$id))
    mix(f$id(t, $N, 4))
    mix(f$id(t, $N, $KD))
    mix(t[1]); mix(t[2]); mix(t["1"]); mix(t[2.5]); mix(t.k1)
    if r == 2 then t = setmetatable(t, { __index = function(_, k) return 0 end }) end
  end
end
run("dm3$id", r$id)
]==], { id = id, N = N(60, 300), R = rnd(3, 5), KD = rnd(1, 12), EX = pick{ "nil", "0/0", "-0.0", "'x'", "1e300" } })
end

-- BUNDLE-T1I (JITDIFF_TSET=t1i draws only this): inline built-in calls of the tier-1 CALL template (`type`, `math.floor/abs/max/min/sqrt`,
-- `string.byte/len`, `select('#')`, `rawlen`) on special operands and odd argument counts, with the global / field replaced (and restored) in the
-- middle of the loop, a call through an alias, multiple results, a hook-free polymorphic site
local T1IT = {}
T1IT[#T1IT + 1] = #TT + 1
TT[#TT + 1] = function(id)
  return tpl([==[
local sel = select
local V$id = { 0, -0.0, 1, -1, 3, 1.5, -1.5, 0.5, 2^53, 2^63, -2^63, 1e300, math.huge, -math.huge, 0/0, math.maxinteger, math.mininteger, "10", "abc", "", true, {}, print, nil }
local S$id = { "", "a", "hello", "\0\255x", ("x"):rep(40), ("y"):rep(41), 5, 1.5, nil, {} }
local I$id = { 1, 2, 3, 0, -1, -2, 40, 41, 1.0, 1.5, "2", math.maxinteger, math.mininteger, true }
local function body$id(i)
  local v, w = V$id[i % #V$id + 1], V$id[(i * 5) % #V$id + 1]
  local s, j = S$id[i % #S$id + 1], I$id[(i * 3) % #I$id + 1]
  local t = {}
  t[1] = type(v)
  t[2] = sel(2, pcall(function() return (math.floor(v)) end))
  t[3] = sel(2, pcall(function() return (math.abs(v)) end))
  t[4] = sel(2, pcall(function() return (math.max(v, w)) end))
  t[5] = sel(2, pcall(function() return (math.min(v, w)) end))
  t[6] = sel(2, pcall(function() return (math.sqrt(v)) end))
  t[7] = sel(2, pcall(function() return (string.byte(s, j)) end))
  t[8] = sel(2, pcall(function() return (string.len(s)) end))
  t[9] = sel(2, pcall(function() return (rawlen(v)) end))
  t[10] = sel(2, pcall(function(...) return (select("#", ...)) end, v, w, s))
  t[11] = sel(2, pcall(function() return (string.byte(s)) end))
  for k = 1, 11 do mix(t[k]) end
  return #tostring(t[1])
end
local function loop$id(n, at, patch, unpatch)
  local acc = 0
  for i = 1, n do
    if i == at then patch() end
    acc = acc + body$id(i)
  end
  unpatch()
  return acc
end
local function nop() end
local function all$id()
  local otype, ofloor, oabs, omax, omin, osqrt, obyte, olen, orawlen, oselect = type, math.floor, math.abs, math.max, math.min, math.sqrt, string.byte, string.len, rawlen, select
  mix(loop$id($N, 0, nop, nop))
  mix(loop$id($N, $K, function() type = function() return "T" end end, function() type = otype end))
  mix(loop$id($N, $K, function() type = math.random end, function() type = otype end))
  mix(loop$id($N, $K, function() math.floor = math.ceil end, function() math.floor = ofloor end))
  mix(loop$id($N, $K, function() math.abs = function() return 0 end end, function() math.abs = oabs end))
  mix(loop$id($N, $K, function() math.max = math.min; math.min = math.max end, function() math.max = omax; math.min = omin end))
  mix(loop$id($N, $K, function() math.sqrt = math.abs end, function() math.sqrt = osqrt end))
  mix(loop$id($N, $K, function() string.byte = string.len end, function() string.byte = obyte end))
  mix(loop$id($N, $K, function() string.len = nil end, function() string.len = olen end))
  mix(loop$id($N, $K, function() rawlen = string.len end, function() rawlen = orawlen end))
  mix(loop$id($N, $K, function() select = rawlen end, function() select = oselect end))
  mix(loop$id($N, 0, nop, nop))
end
run("t1i$id", all$id)
]==], { id = id, N = N(80, 260), K = rnd(3, 60) })
end

-- BUNDLE-A: `#t` on tables that are pushed, popped, holed and refilled in hot loops (the inline length path of tier 1 and of traces:
-- stale hints, holes, hash-part integers, array boundaries, a `__len` appearing mid-loop) and constant-key-0 reads / writes
TT[#TT + 1] = function(id)
  return tpl([==[
local function len$id(n)
  local st, s = {}, 0
  for i = 1, n do
    st[#st + 1] = i * $A
    if i % $B == 0 then st[#st] = nil end
    if i % $C == 0 then st[#st] = nil st[#st] = nil end
    s = s + #st
  end
  local h = { 1, 2, 3, [4] = 4, [5] = $H }
  for i = 1, n do s = s + #h if i == n // 2 then h[#h + 1] = 5 end end
  local ho = {}
  for i = 1, $SZ do ho[i] = i end
  for r = 1, n // $SZ + 1 do
    local k = r % $SZ + 1
    ho[k] = nil
    s = s + #ho
    ho[k] = k
  end
  local z = {}
  for i = 1, n do z[0] = i s = s + z[0] + #z end
  local u = { 1, 2, 3 }
  for i = 1, n do
    if i == n - n // 3 then setmetatable(u, { __len = function() return $L end }) end
    s = s + #u
  end
  return s, #st, #h, #ho
end
run("len$id", len$id, $N)
run("len$id", len$id, $N2)
]==], { id = id, A = rnd(1, 9), B = rnd(2, 7), C = rnd(3, 11), H = rnd(5, 99), SZ = pick{ 3, 4, 7, 8, 9, 16, 17 }, L = rnd(0, 50),
    N = N(60, 400), N2 = N(5, 40) })
end

-- BUNDLE-Q: generic `for` over Lua closure iterators: upvalue state, 1..4 loop variables, an early break, nil / false results, a
-- changing result type, an error inside the iterator, a stateless iterator, a closing value and a coroutine.wrap iterator; the length
-- of the traversal and the iterator behaviour change between rounds
TT[#TT + 1] = function(id)
  return tpl([==[
local lim$id = $N
local function rng$id(n) local i = 0 return function() i = i + 1 if i <= n then return i end end end
local function rng3$id(n) local i = 0 return function() i = i + 1 if i <= n then return i, i * $A, "v" end end end
local function bits$id(n) local i = 0 return function() i = i + 1 if i <= n then return i % $A ~= 0 end end end
local function drift$id(n, at) local i = 0 return function() i = i + 1 if i <= n then if i >= at then return i + 0.5 end return i end end end
local function sless$id(t, i) i = i + 1 local v = t[i] if v ~= nil then return i, v end end
local function bad$id(at) local i = 0 return function() i = i + 1 if i == at then error("bad" .. i) end return i end end
local function clo$id() return setmetatable({}, { __close = function() mix("closed") end }) end
local arr$id = {}
for i = 1, 12 do arr$id[i] = i * $A end
local function all$id()
  for r = 1, $R do
    local n = lim$id - r * 3
    local s = 0
    for x in rng$id(n) do s = s + x end
    mix(s)
    for a, b, c, d in rng3$id(n // 2) do s = s + a + b + #c + (d == nil and 1 or 0) end
    mix(s)
    local t = 0
    for x in rng$id(n) do if x > n - r then break end t = t + x end
    mix(t)
    local f = 0
    for v in bits$id(n) do if v then f = f + 1 end end
    mix(f)
    local d = 0
    for v in drift$id(n, n - r) do d = d + v end
    mix(d)
    for i, v in sless$id, arr$id, 0 do d = d + i + v end
    mix(d)
    if r % 2 == 0 then mix(pcall(function() local q = 0 for x in bad$id(n - r) do q = q + x end return q end)) end
    if r % 3 == 0 then for x in rng$id(5), nil, nil, clo$id() do if x == 3 then break end end end
    if r % 4 == 0 then
      local w = 0
      for x in coroutine.wrap(function() for i = 1, 6 do coroutine.yield(i) end end) do w = w + x end
      mix(w)
    end
    if r == 3 then arr$id[5] = nil end
  end
end
run("q1c$id", all$id)
]==], { id = id, A = rnd(2, 5), N = N(60, 240), R = rnd(4, 7) })
end


-- BUNDLE-B (JITDIFF_TSET=cmp draws only these; not part of TT / T, so the other sets reproduce their old seeds): `<` `<=` `==` (and the swapped
-- / negated forms) on tables with Lua-function `__lt` / `__le` / `__eq`, in loops and in small comparator functions that tier 1 calls
-- or the interpreter calls: results of every type, a metamethod on one operand only, errors and yields thrown inside the metamethod,
-- the metamethod replaced in the middle of the run, mixed operand types, table.sort with and without a comparator
local CMPT = {}
CMPT[#CMPT + 1] = function(id)
  return tpl([==[
local mt$id = {}
local rv$id = { $R1, $R2, $R3 }
local ncall$id = 0
mt$id.__lt = function(a, b) ncall$id = ncall$id + 1; if a.v == $ERRV then error("lt" .. a.v) end return rv$id[1] == "cmp" and a.v < b.v or rv$id[1] end
mt$id.__le = function(a, b) ncall$id = ncall$id + 1; if rv$id[2] == "cmp" then return a.v <= b.v end return rv$id[2] end
mt$id.__eq = function(a, b) ncall$id = ncall$id + 1; if rv$id[3] == "cmp" then return a.v == b.v end return rv$id[3] end
local function new$id(v) return setmetatable({ v = v }, mt$id) end
local plain$id = {}
local function lt$id(a, b) return a < b end
local function le$id(a, b) return a <= b end
local function eq$id(a, b) return a == b end
local function ngt$id(a, b) if not (a > b) then return 1 end return 2 end
local function run$id(n)
  local objs = { new$id(1), new$id(2), new$id(2), new$id(-1), plain$id, 5, "s", $ERRV, new$id($ERRV) }
  local acc = 0
  for i = 1, n do
    local a, b = objs[i % #objs + 1], objs[(i * $A) % #objs + 1]
    if i == n // 3 then rv$id[1], rv$id[2], rv$id[3] = $R2, $R3, "cmp" end
    if i == n // 2 then mt$id.__lt = nil end
    if i == (n * 2) // 3 then mt$id.__lt = function(x, y) return x.v < y.v end; rv$id[1] = "cmp" end
    local ok, r = pcall(lt$id, a, b)
    mix(ok); if ok then mix(r) else err(r) end
    ok, r = pcall(le$id, a, b)
    mix(ok); if ok then mix(r) else err(r) end
    ok, r = pcall(eq$id, a, b)
    mix(ok); if ok then mix(r) else err(r) end
    ok, r = pcall(ngt$id, b, a)
    mix(ok); if ok then mix(r) else err(r) end
    if a == b then acc = acc + 1 end
  end
  mix(acc); mix(ncall$id)
  local lst = {}
  for i = 1, 40 do lst[i] = new$id((i * $A) % 17) end
  rv$id[1] = "cmp"
  mix(pcall(table.sort, lst))
  mix(pcall(table.sort, lst, function(x, y) return x > y end))
  mix(lst[1].v); mix(lst[#lst].v)
end
run("cmp$id", run$id, $N)
]==], { id = id, N = N(150, 400), A = pick{ 3, 5, 7 }, ERRV = pick{ 99, 77 },
        R1 = pick{ '"cmp"', "true", "false", "nil", "0", "{}" }, R2 = pick{ '"cmp"', "true", "false", "nil", "1" }, R3 = pick{ '"cmp"', "true", "false", "nil", '""' } })
end
CMPT[#CMPT + 1] = function(id)
  return tpl([==[
local ymt$id = {}
ymt$id.__lt = function(a, b) local r = coroutine.yield(a.v) ; return r end
ymt$id.__le = function(a, b) coroutine.yield("le"); return a.v <= b.v end
ymt$id.__eq = function(a, b) local r = coroutine.yield("eq"); return r end
local function y$id(v) return setmetatable({ v = v }, ymt$id) end
local function body$id(n)
  local p, q = y$id(1), y$id(2)
  local acc = 0
  for i = 1, n do
    if p < q then acc = acc + 1 end
    if p <= q then acc = acc + 10 end
    if p == q then acc = acc + 100 end
    if not (q < p) then acc = acc + 1000 end
  end
  return acc
end
local function run$id(n)
  local co = coroutine.wrap(function() return body$id(n) end)
  local steps, v = 0, co()
  while true do
    steps = steps + 1
    if type(v) == "number" and v > 5 then mix(steps); return v end
    v = co((steps % $M) == 0)
  end
end
run("cmpy$id", run$id, $N)
]==], { id = id, N = N(40, 120), M = pick{ 2, 3, 5 } })
end

-- BUNDLE-C (JITDIFF_TSET=bc draws only these; not part of TT / T): recursion shapes of the per-site rules. `sbump`: a recursive walk over a tree of
-- splits whose depth varies by data (regex_nfa's addState: an on-list check, two recursive calls, a leaf append), restructured in the middle of the run, an
-- error thrown at depth, a yield inside the recursion. `fnpromo`: a long loop-free function entered from a tail-recursive driver and from a loop,
-- its table changing types and a callee replaced in the middle.
local BCT = {}
BCT[#BCT + 1] = function(id)
  return tpl([==[
local states$id = {}
local function node$id(d)
  local s = { kind = "leaf" }
  states$id[#states$id + 1] = s
  local me = #states$id
  if d > 0 then
    s.kind = "split"
    s.o1 = node$id(d - 1)
    s.o2 = (d % $K == 0) and node$id(d - 1) or node$id(rnd0$id(d))
  end
  return me
end
function rnd0$id(d) return d % 3 end
local root$id = node$id($D)
local function add$id(list, onl, s, gen, lim)
  if onl[s] == gen then return end
  onl[s] = gen
  local st = states$id[s]
  if st.kind == "split" then
    add$id(list, onl, st.o1, gen, lim)
    add$id(list, onl, st.o2, gen, lim)
  else
    list[#list + 1] = s
    if lim and #list > lim then error("deep$id") end
  end
end
local function run$id(n)
  local onl, total = {}, 0
  for gen = 1, n do
    local list = {}
    add$id(list, onl, root$id, gen)
    total = total + #list
    if gen == $B then
      local s = states$id[root$id]
      if s.o2 then states$id[s.o2].kind = "leaf" end
    end
  end
  return total
end
local function err$id(n)
  local onl = {}
  return select(2, pcall(function() for gen = 1, n do add$id({}, onl, root$id, gen, 3) end end))
end
local function gen$id(n)
  local co = coroutine.wrap(function()
    local onl = {}
    for gen = 1, n do
      local list = {}
      add$id(list, onl, root$id, gen)
      coroutine.yield(#list)
    end
    return -1
  end)
  local s = 0
  for i = 1, n do local v = co(); if v < 0 then break end; s = s + v end
  return s
end
run("bcs$id", function(n) mix(err$id(5)); mix(gen$id(n // 4)); return run$id(n) end, $N)
]==], { id = id, N = N(200, 900), D = rnd(3, 6), K = pick{ 2, 3 }, B = rnd(40, 150) })
end
BCT[#BCT + 1] = function(id)
  return tpl([==[
local t$id = {}
for i = 1, 64 do t$id[i] = { k = i * 7 % 97, j = i % 13, w = (i % 7) * 0.5 } end
local function work$id(i)
  local a, b = t$id[i % 64 + 1], t$id[(i * 3) % 64 + 1]
  local x = a.k * 3 + a.j * 2 + a.w
  local y = b.k * 3 + b.j * 2 + b.w
  if x > y then x, y = y, x end
  a.w = (a.w + 0.25) % 4
  return x + y * 0.5 + (b.j - a.j)
end
local function drive$id(n, acc)
  if n == 0 then return acc end
  return drive$id(n - 1, acc + work$id(n))
end
local function loop$id(n)
  local acc = 0
  for r = 1, n do
    if r == $B then for i = 1, 64 do t$id[i].k = t$id[i].k + 0.5 end end
    if r == $C then work$id = function(i) return i % 5 end end
    acc = acc + drive$id($M, 0) + work$id(r)
  end
  return acc
end
run("bcf$id", loop$id, $N)
]==], { id = id, N = N(300, 1500), M = rnd(5, 40), B = rnd(100, 250), C = rnd(260, 600) })
end

-- BUNDLE-GM (JITDIFF_TSET=gm draws only these; not part of TT / T): `gmatch` iterators in generic-for position (the `gmatch` limit): one capture / whole
-- match / several captures / position captures, empty matches, `init`, short and long results mixed, early breaks, iterator reuse after exhaustion, a
-- pattern error in the middle of a loop, a yield inside the loop, subjects whose length changes between calls.
local GMT = {}
GMT[#GMT + 1] = function(id)
  local pats = { "%a+", "%d+", "(%a)%a*", "[^ ]+", "%S+", "(%w+)", "x*", "(%a)(%d)", "()%a", "%a*", "[ab]", "(.-),", "(%d%d?)", "^%a+" }
  return tpl([==[
local subj$id = { "alpha beta  gamma 12 34 x1 y22", "", "one", ("w"):rep($L) .. " a " .. ("z"):rep($L + 3), "a1,b2,,c3,", "ab ab ba ab" }
local pat$id = { $P }
local function run$id(n)
  local acc = 0
  for i = 1, n do
    local s, p = subj$id[i % #subj$id + 1], pat$id[i % #pat$id + 1]
    local c = 0
    for a, b in s:gmatch(p) do
      c = c + 1
      acc = (acc * 31 + #tostring(a) + (b and 7 or 0)) % 1000003
      if c > $K then break end
    end
    for w in s:gmatch(p, i % 5 - 1) do acc = acc + #tostring(w) end
    for w in s:gmatch(p, -(i % 4)) do acc = acc + 1 end
  end
  return acc
end
local function reuse$id(n)
  local acc = 0
  for i = 1, n do
    local it = subj$id[i % #subj$id + 1]:gmatch(pat$id[i % #pat$id + 1])
    for w in it do acc = acc + #tostring(w); if acc % 5 == 0 then break end end
    for w in it do acc = acc + 1 end
    acc = acc + (it() and 1 or 0)
  end
  return acc
end
run("gma$id", run$id, $N)
run("gmb$id", reuse$id, $N)
]==], { id = id, N = N(300, 1200), L = rnd(30, 60), K = rnd(2, 6),
    P = table.concat((function() local t = {} for i = 1, rnd(2, 4) do t[#t + 1] = string.format("%q", pats[rnd(#pats)]) end return t end)(), ", ") })
end
GMT[#GMT + 1] = function(id)
  return tpl([==[
local bad$id = { "[a", "%", "(a", "%1", "(()", "abc", ".-%b" }
local function errl$id(n)
  local r = {}
  for i = 1, n do
    local p = bad$id[(i * $M) % #bad$id + 1]
    local ok, e = pcall(function()
      local c = 0
      for w in ("abc abc def"):gmatch(p) do c = c + 1 end
      return c
    end)
    r[#r + 1] = ok and "t" or tostring(e):gsub("^.-:%d+: ", "")
  end
  return table.concat(r, ";")
end
local function co$id(n)
  local co = coroutine.wrap(function(s)
    for w in s:gmatch("%a+") do coroutine.yield(#w) end
    return -1
  end)
  local t, first = 0, co(("aa bbb c dddd e"):rep(3, " "))
  for i = 1, n do
    local v = co()
    if v == nil or v < 0 then break end
    t = t + v
  end
  return t + first
end
local function chg$id(n)
  local s, acc = "x1 x2 x3", 0
  for i = 1, n do
    for w in s:gmatch("x%d") do s = s .. (i % 3 == 0 and "!" or ""); acc = acc + #w end
    if #s > 40 then s = "x1 x2 x3" end
  end
  return acc
end
run("gme$id", errl$id, $N)
run("gmc$id", co$id, 12)
run("gmd$id", chg$id, $N)
]==], { id = id, N = N(100, 600), M = rnd(1, 6) })
end

-- P3 (JITDIFF_TSET=pd draws only these; not part of TT / T, so the other sets reproduce their old seeds): polymorphic call sites. A table of K distinct closures
-- called through one site in random / round-robin / skewed order (`ncallee`: the callee guard leaves the entry segment), and a toy interpreter whose word
-- distribution changes mid-run (`chainre`: chains of side traces re-ordered by hotness), with the callee table replaced under the running loop.
local PDT = {}
PDT[#PDT + 1] = function(id)
  return tpl([==[
local fs$id = {}
for i = 1, $K do fs$id[i] = load(("local d = ...; return function(x) return (x * %d + %d) %% 1000003 end"):format(i % 3 + 1, i))() end
local ord$id = {}
do
  local s = $S
  for i = 1, $L do
    s = (s * 16807) % 2147483647
    local r = s // 64
    if $MODE == 0 then ord$id[i] = r % $K + 1
    elseif $MODE == 1 then ord$id[i] = (i - 1) % $K + 1
    else ord$id[i] = (r % 17 == 0) and (r % $K + 1) or 1 end
  end
end
local function disp$id(n)
  local acc = 0
  for r = 1, n do
    for i = 1, $L do acc = fs$id[ord$id[i]](acc) end
    if r == $SW then
      for i = 1, $K do fs$id[i] = load(("return function(x) return (x + %d) %% 1000003 end"):format(i * 5))() end
    end
  end
  return acc
end
run("pda$id", disp$id, $N)
]==], { id = id, K = rnd(2, 12), S = rnd(1, 1000000), L = rnd(64, 400), MODE = rnd(0, 2), N = N(20, 120), SW = rnd(5, 15) })
end
PDT[#PDT + 1] = function(id)
  return tpl([==[
local words$id = {}
for i = 1, $K do
  words$id[i] = { kind = "prim", fn = load(("return function() return %d end"):format(i * 3 % 7 + i))(), id = i }
end
local vword$id = { kind = "var", addr = 5 }
local acc$id = 0
local function exec$id(word)
  if word.kind == "prim" then
    acc$id = (acc$id + word.fn()) % 1000003
  elseif word.kind == "var" then
    acc$id = acc$id + word.addr
  end
end
local seed$id = $S
local function mk$id(phase)
  local prog = {}
  for i = 1, $L do
    seed$id = (seed$id * 16807) % 2147483647
    local r = seed$id // 16
    local w
    if phase == 0 then w = words$id[(i - 1) % $K + 1]
    elseif r % 16 == 0 then w = vword$id
    elseif r % 1000 < 3 then w = words$id[(r // 1000) % $K + 1]
    elseif phase == 1 then w = words$id[$K - (r // 7) % math.min($K, 4)]
    else w = words$id[1 + (r // 7) % math.min($K, 4)] end
    prog[i] = w
  end
  return prog
end
local function interp$id(n)
  acc$id = 0
  for phase = 0, 2 do
    local prog = mk$id(phase)
    for rep = 1, (phase == 0) and 2 or n do
      for i = 1, #prog do exec$id(prog[i]) end
    end
  end
  return acc$id
end
run("pdb$id", interp$id, $N)
]==], { id = id, K = rnd(3, 12), S = rnd(1, 1000000), L = rnd(100, 600), N = N(20, 80) })
end

-- BUNDLE-A (JITDIFF_TSET=ba draws only these; not part of TT / T): `tonumber(string)`, `next(t [, k])` called directly and plain `string.find`
-- (the `libnum` / `libnext` / `libfind` limits): numerals of every syntax mixed with non-numerals, tables that change shape under the traversal, specials
-- appearing in patterns at run time, negative / over-length / nil starts, empty patterns, found / not found alternation, long subjects.
local BAT = {}
BAT[#BAT + 1] = function(id)
  return tpl([==[
local nums$id = { "10", " 12 ", "0x1F", "1e2", "3.5", "9223372036854775807", "9223372036854775808", "-0x10", "  7", "1e", "abc", "", "5 5", ".5", "0x.8p1", "1_0" }
local function tn$id(n)
  local si, sf, cnt, nn = 0, 0.0, 0, 0
  for i = 1, n do
    local v = tonumber(nums$id[(i * $M) % $Q + 1])
    if v then cnt = cnt + 1; if math.type(v) == "integer" then si = si + v else sf = sf + v end else nn = nn + 1 end
  end
  return si, sf, cnt, nn
end
local function tn2$id(n)
  local a = 0
  for i = 1, n do a = a + tonumber(i < $C and "5" or "5.5") + tonumber(" 0x10 ") end
  return a
end
local ok$id, r$id = pcall(function() local a = 0 for i = 1, $N do a = a + tonumber(i < $C and "7" or "zz") end return a end)
run("bna$id", tn$id, $N)
run("bnb$id", tn2$id, $N)
H = (H * 31 + #tostring(ok$id) + #tostring(r$id)) % 1000003
]==], { id = id, N = N(300, 1500), M = rnd(1, 7), Q = rnd(2, 16), C = rnd(280, 900) })
end
BAT[#BAT + 1] = function(id)
  return tpl([==[
local shapes$id = { {}, { 5, 6 }, { a = 3 }, { 1, 2, x = 5, y = 6, 7 }, { [10] = 1, [20] = 2 }, { "s", [true] = 4 } }
local function nx$id(n)
  local acc = 0
  for i = 1, n do
    local t = shapes$id[i % #shapes$id + 1]
    local k, v = next(t)
    while k do acc = acc + (tonumber(v) or 1); k, v = next(t, k) end
    if next(t) == nil then acc = acc + 1 end
  end
  return acc
end
local function del$id(n)
  local acc = 0
  for i = 1, n do
    local t = { a = 1, b = 2, c = 3, 1, 2, [$K] = 4 }
    local k, v = next(t)
    while k do acc = acc + v; t[k] = nil; k, v = next(t, k) end
  end
  return acc
end
local function bad$id(n)
  local c = 0
  for i = 1, n do
    local ok = pcall(next, shapes$id[2], i % $M == 0 and "nokey" or 1)
    c = c + (ok and 1 or 0)
  end
  return c
end
local function repl$id(n)
  local c = 0
  local real = next
  for i = 1, n do
    if i == $C then next = function() return 77, 1 end end
    if i == $C + 40 then next = rawlen end
    if i == $C + 80 then next = real end
    local k = next({ 5, 6, 7 })
    c = c + (k or 0)
  end
  next = real
  return c
end
local fns$id = { next, rawlen }
local function via$id(n)
  local c = 0
  for i = 1, n do local k = fns$id[1 + (i // $C) % 2]({ 7, 8, 9 }); c = c + (k or 0) end
  return c
end
run("bxe$id", via$id, $N)
run("bxa$id", nx$id, $N)
run("bxb$id", del$id, $N)
run("bxc$id", bad$id, $N)
run("bxd$id", repl$id, $N)
]==], { id = id, N = N(300, 1200), K = rnd(5, 30), M = rnd(2, 9), C = rnd(280, 800) })
end
BAT[#BAT + 1] = function(id)
  return tpl([==[
local txt$id = "hello world, hello lua " .. ("ab"):rep($L)
local pats$id = { "world", "lua", "zzz", "", "o", "l.a", "%s", "b" .. ("a"):rep(2) }
local function fd$id(n)
  local a = 0
  for i = 1, n do
    local p = pats$id[i % #pats$id + 1]
    local init = ({ 1, -5, -1000, 5, #txt$id, #txt$id + 1, #txt$id + 2, 0 })[i % 8 + 1]
    local plain = ({ true, false, nil, 1, "x" })[i % 5 + 1]
    local x, y = txt$id:find(p, init, plain)
    a = (a * 7 + (x or -1) + (y or -2)) % 1000003
    x, y = string.find(txt$id, p, nil, true)
    a = a + (x or 0) + (y or 0)
  end
  return a
end
local zl$id = { "zzz", "l+" }
local function fp$id(n)
  local a = 0
  for i = 1, n do
    local x, y = txt$id:find(i < $C and "lo" or (i % 2 == 0 and "l(o)" or "lo"), 1, i % 3 == 0)
    a = a + (x or -1) + (y or -2)
    local z = txt$id:find("hello", 1, true)
    a = a + z
    local w1, w2 = txt$id:find(zl$id[1 + (i // $C) % 2], 1)
    a = a + (w1 or -3) + (w2 or -4)
  end
  return a
end
local long$id = ("abc"):rep($L) .. "needle" .. ("xyz"):rep($L)
local function fl$id(n)
  local a = 0
  for i = 1, n do
    local x, y = long$id:find(i % 2 == 0 and "needle" or ("abc"):rep(3), 1, true)
    a = a + (x or -1) + (y or -2)
    local m = select("#", long$id:find("q", 1, true))
    a = a + m
  end
  return a
end
run("bfa$id", fd$id, $N)
run("bfb$id", fp$id, $N)
run("bfc$id", fl$id, $N)
]==], { id = id, N = N(300, 1200), L = rnd(20, 60), C = rnd(280, 800) })
end

-- BUNDLE-C (JITDIFF_TSET=cc draws only these; not part of TT / T): `string.format` of constant `%d %x %X %s %c` formats with widths and flags (the `tfmt`
-- limit): the format register changing between constants, arguments leaving the recorded shape (floats, strings, nil, missing / extra), a string / number
-- metatable `__tostring` appearing late, results of both string kinds; and calls of light C functions from tier 1 (the `t1ccall` limit) with every wanted
-- result count, errors through `pcall`, a callee that changes, a hook set in the middle.
local CCT = {}
CCT[#CCT + 1] = function(id)
  return tpl([==[
local fm$id = { "%d", "%5d", "%-6d|", "%05d", "%x", "%08X", "%c", "[%s]", "%10s", "%s:%d", "%08x|%-6d|%5s", "%d %x %X" }
local function ff$id(n)
  local h = 0
  for i = 1, n do
    local f = fm$id[(i // $Q) % #fm$id + 1]
    local ok, r = pcall(string.format, f, i * $M - 100, i, "ab")
    h = (h * 31 + #tostring(r) + (ok and 1 or 0)) % 1000003
  end
  return h
end
local function fc$id(n)
  local h = 0
  for i = 1, n do
    h = h + #string.format("%08x|%-6d|%5s", i * $M, i, i % 3 == 0 and "abcdefg" or "ab")
    h = h + #string.format("%s-%s", i % $Q == 0 and ("z"):rep(50) or "k", i)
    h = h + #string.format(i < $C and "%d" or "%d:%d", i, i)
    local s = string.format("%X", i * $M - 5000)
    h = h + s:byte(1)
  end
  return h
end
local function fb$id(n, v)
  local h = 0
  for i = 1, n do
    local ok, r = pcall(string.format, "%d|%s", i < $C and i or v, i < $C and "s" or v)
    h = h + #tostring(r) + (ok and 1 or 0)
  end
  return h
end
run("cca$id", ff$id, $N)
run("ccb$id", fc$id, $N)
run("ccc$id", fb$id, $N, $V)
]==], { id = id, N = N(300, 1200), M = rnd(1, 99991), Q = rnd(7, 60), C = rnd(150, 800), V = pick({ "5", "5.0", "5.5", "'12'", "nil", "{}", "'a\\0b'", "true", "math.huge" }) })
end
CCT[#CCT + 1] = function(id)
  return tpl([==[
local function fm$id(n)
  local h = 0
  for i = 1, n do
    if i == $C then getmetatable("").__tostring = function(s) return "T" .. s end end
    h = h + #string.format("%s|%d", "x", i)
    if i == $C2 then getmetatable("").__tostring = nil; debug.setmetatable(0, { __tostring = function(v) return "N" end }) end
    h = h + #string.format("%s", i)
    if i == $C3 then debug.setmetatable(0, nil) end
  end
  getmetatable("").__tostring = nil
  debug.setmetatable(0, nil)
  return h
end
run("ccd$id", fm$id, $N)
]==], { id = id, N = N(300, 1200), C = rnd(60, 150), C2 = rnd(160, 300), C3 = rnd(310, 450) })
end
CCT[#CCT + 1] = function(id)
  return tpl([==[
local find$id, sub$id, byte$id, sel$id = string.find, string.sub, string.byte, select
local function cw$id(n)
  local h = 0
  for i = 1, n do
    find$id("hello world", "o w")
    local a = sub$id("hello", 1 + i % 3, 4)
    local b, c = find$id("hello world", i % 2 == 0 and "wor" or "lo")
    local d, e, g = byte$id("abc", 1, 3)
    local t = { byte$id("abcd", 1, -1) }
    h = h + #a + (b or 0) + (c or 0) + d + e + g + #t + sel$id("#", find$id("x", "y"))
  end
  return h
end
local fs$id = { sub$id, string.upper, function(s) return s .. "!" end, tostring, string.lower }
local function cp$id(n)
  local h = 0
  for i = 1, n do h = h + #fs$id[(i // $Q) % 5 + 1]("Hello") end
  return h
end
local function ce$id(n)
  local h = 0
  for i = 1, n do
    local ok, m = pcall(i < $C and sub$id or find$id, i < $C and "abc" or nil, 1, 2)
    h = h + #tostring(m) + (ok and 1 or 0)
  end
  return h
end
local hk$id = 0
local function ch$id(n)
  local h = 0
  for i = 1, n do
    h = h + #sub$id("hello", 1, 2)
    if i == $C then debug.sethook(function() hk$id = hk$id + 1 end, "", 17) end
  end
  debug.sethook()
  return h
end
run("cce$id", cw$id, $N)
run("ccf$id", cp$id, $N)
run("ccg$id", ce$id, $N)
run("cch$id", ch$id, $N)
H = (H * 31 + (hk$id > 0 and 1 or 0)) % 1000003
]==], { id = id, N = N(300, 1200), Q = rnd(7, 60), C = rnd(100, 500) })
end

-- BUNDLE-D (JITDIFF_TSET=bd draws only these; not part of TT / T, so the other sets reproduce their old seeds): arithmetic metamethods in hot
-- loops and in small functions that tier 1 runs (`armm`), and metamethod / pcall nesting to the C-stack limit (`ccalls`): `+ - * / % // ^` `..`
-- unary minus and `#` on tables with Lua-function metamethods whose results have every type, metamethods on one operand only, plain tables
-- and nil / string operands (the interpreter's error), errors and yields thrown inside the metamethod, the metamethod replaced in the middle
-- of the run, tail-call and plain returns, deep recursion through __index / __add / pcall.
local BDT = {}
BDT[#BDT + 1] = function(id)
  return tpl([==[
local amt$id = {}
local rv$id = { $R1, $R2, $R3 }
local ncall$id = 0
amt$id.__add = function(a, b) ncall$id = ncall$id + 1; if rawequal(a, b) and rv$id[1] == "err" then error("add" .. ncall$id) end
  if rv$id[1] == "cmp" then return (type(a) == "table" and a.v or a) + (type(b) == "table" and b.v or b) end return rv$id[1] end
amt$id.__sub = function(a, b) ncall$id = ncall$id + 1; if rv$id[2] == "cmp" then return (type(a) == "table" and a.v or a) - (type(b) == "table" and b.v or b) end return rv$id[2] end
amt$id.__mul = function(a, b) ncall$id = ncall$id + 1; return rv$id[3] == "cmp" and 2 * (type(a) == "table" and a.v or a) or rv$id[3] end
amt$id.__unm = function(a) ncall$id = ncall$id + 1; if rv$id[2] == "cmp" then return -a.v end return rv$id[3] end
amt$id.__div = function(a, b) ncall$id = ncall$id + 1; return setmetatable({ v = a.v }, amt$id) end
amt$id.__mod = function(a, b) return 7 end
amt$id.__idiv = function(a, b) return nil end
amt$id.__band = function(a, b) return a.v end
amt$id.__shl = function(a, b) return 3 end
local function new$id(v) return setmetatable({ v = v }, amt$id) end
local plain$id = {}
local function add$id(a, b) return a + b end
local function sub$id(a, b) return a - b end
local function mul$id(a, b) return a * b end
local function unm$id(a) return -a end
local function div$id(a, b) return a / b end
local function chain$id(a, b, c) return a + b * c - a end
local function run$id(n)
  local objs = { new$id(1), new$id(2), new$id(-3), plain$id, 5, 2.5, "s", "10", false, new$id(0) }
  local acc = 0
  for i = 1, n do
    local a, b = objs[i % #objs + 1], objs[(i * $A) % #objs + 1]
    if i == n // 3 then rv$id[1], rv$id[2], rv$id[3] = $R2, $R3, "cmp" end
    if i == n // 2 then amt$id.__add = nil end
    if i == (n * 2) // 3 then amt$id.__add = function(x, y) return 4 end; rv$id[1] = "cmp" end
    local ok, r = pcall(add$id, a, b)
    mix(ok); if ok then mix(r) else err(r) end
    ok, r = pcall(sub$id, a, 3)
    mix(ok); if ok then mix(r) else err(r) end
    ok, r = pcall(mul$id, 1.5, b)
    mix(ok); if ok then mix(r) else err(r) end
    ok, r = pcall(unm$id, a)
    mix(ok); if ok then mix(r) else err(r) end
    ok, r = pcall(div$id, a, b)
    mix(ok); if ok and type(r) == "table" then mix(r.v) elseif ok then mix(r) else err(r) end
    ok, r = pcall(chain$id, a, b, objs[(i + 1) % #objs + 1])
    mix(ok); if ok then mix(r) else err(r) end
    if i % 5 == 0 then
      ok, r = pcall(function() return a % b, a // b, a & b, a << 1 end)
      mix(ok); if ok then mix(r) else err(r) end
    end
    local x = objs[1]
    local y = x + objs[2]
    local z = -x
    acc = acc + (type(y) == "number" and y or 1) + (type(z) == "number" and z or 1)
  end
  mix(acc); mix(ncall$id)
end
run("am$id", run$id, $N)
]==], { id = id, N = N(150, 500), A = pick{ 3, 5, 7 },
        R1 = pick{ '"cmp"', "1", "false", "nil", "0.5", "{}", '"str"' }, R2 = pick{ '"cmp"', "true", "false", "nil", "1" }, R3 = pick{ '"cmp"', "true", "false", "nil", '""' } })
end
BDT[#BDT + 1] = function(id)
  return tpl([==[
local vt$id = {}
local function vec$id(x, y) return setmetatable({ x = x, y = y }, vt$id) end
vt$id.__add = function(a, b) return vec$id(a.x + b.x, a.y + b.y) end
vt$id.__sub = function(a, b) local r = vec$id(a.x - b.x, a.y - b.y); return r end
vt$id.__mul = function(a, b)
  if type(a) == "number" then return vec$id(a * b.x, a * b.y) elseif type(b) == "number" then return vec$id(a.x * b, a.y * b) end
  return a.x * b.x + a.y * b.y
end
vt$id.__unm = function(a) return vec$id(-a.x, -a.y) end
vt$id.__div = function(a, b) return vec$id(a.x / b, a.y / b) end
vt$id.__len = function(a) return 2 end
vt$id.__concat = function(a, b) return "vec" end
local function run$id(n)
  local p, v = vec$id(0, 0), vec$id(1, $K)
  local acc = 0
  for i = 1, n do
    p = p + v * 0.5 - (-v) / 4
    local d = p - v
    acc = acc + d * d + #p
    if i % $M == 0 then p = vec$id(i % 7, 0); v = vec$id(1, i % 3) end
    if i == n // 2 then vt$id.__mul = function(a, b) return 1 end end
  end
  mix(acc); mix(p.x); mix(p.y)
end
run("vc$id", run$id, $N)
]==], { id = id, N = N(150, 500), K = pick{ 1, 2, 3 }, M = pick{ 7, 13, 50 } })
end
BDT[#BDT + 1] = function(id)
  return tpl([==[
local ym$id = {}
ym$id.__add = function(a, b) local r = coroutine.yield(a.v) ; return r end
ym$id.__unm = function(a) coroutine.yield("unm"); return a.v end
ym$id.__sub = function(a, b) if b == 0 then error("sub0") end coroutine.yield("sub"); return a.v - b end
local function y$id(v) return setmetatable({ v = v }, ym$id) end
local fin$id
local function body$id(n)
  local p, q = y$id(1), y$id(2)
  local acc = 0
  for i = 1, n do
    local s = p + q
    if type(s) == "number" then acc = acc + s end
    acc = acc + (-p)
    acc = acc + (p - 3)
    local ok, e = pcall(function() return p - 0 end)
    if not ok then acc = acc + #e end
  end
  fin$id = acc
  return acc
end
local function run$id(n)
  local co = coroutine.wrap(function() return body$id(n) end)
  local steps, v = 0, co()
  while not fin$id do
    steps = steps + 1
    v = co(steps % $M)
  end
  mix(steps); mix(fin$id)
end
run("ay$id", run$id, $N)
]==], { id = id, N = N(30, 100), M = pick{ 2, 3, 5 } })
end
BDT[#BDT + 1] = function(id)
  return tpl([==[
-- C-stack depth: recursion through metamethods and pcall near LUAI_MAXCCALLS, after a hot shallow phase
local dm$id = {}
dm$id.__index = function(t, k) if k <= 0 then return 0 end return t[k - 1] + 1 end
dm$id.__add = function(a, b) if a.n <= 0 then return b end return setmetatable({ n = a.n - 1 }, dm$id) + b end
dm$id.__unm = function(a) if a.n <= 0 then return 0 end return -setmetatable({ n = a.n - 1 }, dm$id) - 1 end
dm$id.__lt = function(a, b) if a.n <= 0 then return true end return setmetatable({ n = a.n - 1 }, dm$id) < b end
local obj$id = setmetatable({}, dm$id)
local function probe$id(kind, d)
  local ok, v
  if kind == 1 then ok, v = pcall(function() return obj$id[d] end)
  elseif kind == 2 then ok, v = pcall(function() return setmetatable({ n = d }, dm$id) + 1 end)
  elseif kind == 3 then ok, v = pcall(function() return -setmetatable({ n = d }, dm$id) end)
  elseif kind == 4 then ok, v = pcall(function() return setmetatable({ n = d }, dm$id) < 1 end)
  else
    local function rec(k) if k <= 0 then return 0 end local o, r = pcall(rec, k - 1); if not o then error(r, 0) end return r + 1 end
    ok, v = pcall(rec, d)
  end
  return ok, ok and v or tostring(v):gsub("0x%x+", "P"):sub(1, 60)
end
local function run$id(n)
  for i = 1, n do
    local ok, v = probe$id(i % 5 + 1, i % 9)
    mix(ok); mix(v)
  end
  for _, d in ipairs{ 20, 60, 100, 150, 180, 190, 194, 196, 197, 198, 199, 200, 201, 202, 203, 210, 260 } do
    for kind = 1, 5 do
      local ok, v = probe$id(kind, d + $O)
      mix(ok); mix(v)
    end
  end
  for i = 1, n do
    local ok, v = probe$id(i % 5 + 1, i % 7)
    mix(ok); mix(v)
  end
end
run("cs$id", run$id, $N)
]==], { id = id, N = N(150, 500), O = pick{ 0, 0, 1, -1, 3 } })
end

-- BUNDLE-F (JITDIFF_TSET=bf draws only these; not part of TT / T, so the other sets reproduce their old seeds): `stitch0` (loops whose side traces
-- start at a C call: several C functions, flipping branches) and `fibdiet` (tree recursion, recursion deep under pcall (the overflow limit itself is tr_core::bf_rec_deep: too slow here),
-- recursion inside coroutines, with one or two live locals per frame).
local BFT = {}
BFT[#BFT + 1] = function(id)
  return tpl([==[
local bfc$id = { math.ult, string.byte, select, math.abs, rawlen, type }
local bft$id = { 1, 2, 3, 4, 5, 6, 7, 8 }
local function bfrun$id(n, mode)
  local a, b, c, d = 0, 1, 2, 3
  for i = 1, n do
    a = (a + bft$id[1] * i) & 0xffff; b = (b + bft$id[2] + a) & 0xffff; c = (c + (a ~ b)) & 0xffff; d = (d + bft$id[3] + c) & 0xffff
    a = (a + bft$id[4] * i) & 0xffff; b = (b + bft$id[5] + a) & 0xffff; c = (c + (a ~ b)) & 0xffff; d = (d + bft$id[6] + c) & 0xffff
    if i % $M1 == 0 then a = a + (math.ult(i, 5) and 1 or 0) end
    if mode == 1 or i % $M2 == 0 then d = d + (string.byte("abc", i % 3 + 1) or 0) end
    if i % $M3 == 0 then c = c + select("#", i, a, b) end
    if i % 97 == 0 then c = c + (type(a) == "number" and 1 or 0) + rawlen({ i }) end
    if i % 500 == 499 then pcall(error, "x") end
  end
  return a + b + c + d
end
run("bf$id", function(n) mix(bfrun$id(n, 0)); mix(bfrun$id(n, 1)) end, $N)
]==], { id = id, N = N(300, 1500), M1 = pick{ 1, 2, 3 }, M2 = pick{ 2, 3, 5 }, M3 = pick{ 4, 7 } })
end
BFT[#BFT + 1] = function(id)
  return tpl([==[
local function bfib$id(n) if n < 2 then return n end return bfib$id(n - 1) + bfib$id(n - 2) end
local function bfwalk$id(n, acc) if n == 0 then return acc end local x = acc * 3 + n; return bfwalk$id(n - 1, x % 1000003) + 1 end
local function bfdown$id(n) if n == 0 then return 0 end return 1 + bfdown$id(n - 1) end
run("bfr$id", function(n)
  for i = 1, n do mix(bfib$id(8 + i % 5)); mix(bfwalk$id(i % 40, i)) end
  for d = 150, 215, $S do
    local ok, v = pcall(bfdown$id, d)
    mix(ok); mix(ok and v or #tostring(v) > 0)
  end
  local co = coroutine.wrap(function(k) for j = 1, 3 do coroutine.yield(bfib$id(k + j)) end return bfdown$id(k) end)
  mix(co(6)); mix(co()); mix(co()); mix(co())
end, $N)
]==], { id = id, N = N(20, 60), S = pick{ 3, 5, 9 } })
end

-- BUNDLE-G (JITDIFF_TSET=bg draws only these; not part of TT / T, so the other sets reproduce their old seeds): wide polymorphic call sites for `chaindepth`
-- (K = 3..40 closures or classes behind one site, random / round-robin / skewed / one-phase-then-another order, the table or the classes replaced under the loop).
local BGT = {}
BGT[#BGT + 1] = function(id)
  return tpl([==[
local fs$id = {}
local function fill$id(off) for i = 1, $K do fs$id[i] = load(("return function(x) return (x * %d + %d) %% 1000003 end"):format(i % 3 + 1, i + off))() end end
fill$id(0)
local ord$id = {}
do
  local s = $S
  for i = 1, $L do
    s = (s * 16807) % 2147483647
    local r = s // 64
    if $MODE == 0 then ord$id[i] = r % $K + 1
    elseif $MODE == 1 then ord$id[i] = (i - 1) % $K + 1
    elseif $MODE == 2 then ord$id[i] = (r % 9 == 0) and (r % $K + 1) or 1
    else ord$id[i] = (i * 7 // $L == 0) and (r % $K + 1) or (r % 3 + 1) end
  end
end
local function disp$id(n)
  local acc = 0
  for r = 1, n do
    for i = 1, $L do acc = fs$id[ord$id[i]](acc) end
    if r == $SW then fill$id(50) end
  end
  return acc
end
run("bga$id", disp$id, $N)
]==], { id = id, K = rnd(3, 40), S = rnd(1, 1000000), L = rnd(64, 400), MODE = rnd(0, 3), N = N(20, 100), SW = rnd(5, 15) })
end
BGT[#BGT + 1] = function(id)
  return tpl([==[
local cls$id = {}
local function mkcls$id(off)
  for i = 1, $K do
    local C = {}
    C.__index = C
    C.step = load(("return function(self, a) self.v = (self.v + a + %d) %% 1000003 return self.v end"):format(i + off))()
    cls$id[i] = C
  end
end
mkcls$id(0)
local objs$id = {}
do
  local s = $S
  for i = 1, $L do
    s = (s * 16807) % 2147483647
    local r = s // 64
    local c = ($MODE == 0) and (r % $K + 1) or (($MODE == 1) and ((i - 1) % $K + 1) or ((r % 8 == 0) and (r % $K + 1) or 1))
    objs$id[i] = setmetatable({ v = i }, cls$id[c])
  end
end
local function meth$id(n)
  local acc = 0
  for r = 1, n do
    for i = 1, $L do acc = (acc + objs$id[i]:step(r)) % 1000003 end
    if r == $SW then
      mkcls$id(9)
      for i = 1, $L, 3 do setmetatable(objs$id[i], cls$id[(i % $K) + 1]) end
    end
  end
  return acc
end
run("bgb$id", meth$id, $N)
]==], { id = id, K = rnd(3, 40), S = rnd(1, 1000000), L = rnd(64, 400), MODE = rnd(0, 2), N = N(20, 100), SW = rnd(5, 15) })
end

-- BUNDLE-J (JITDIFF_TSET=bj draws only these; not part of TT / T): Lua functions called from C (the `centry` limit): sort comparators over objects with `__lt`
-- metamethods and over plain values, `gsub` callbacks that change type, `pcall` / `xpcall` of functions that raise, `coroutine.wrap` bodies, errors and yields
-- (illegal ones) across the C call, a function whose upvalue / callee is replaced between calls.
local BJT = {}
BJT[#BJT + 1] = function(id)
  return tpl([==[
local ob$id = {}
ob$id.__index = ob$id
ob$id.__lt = function(a, b) return a:key() < b:key() end
ob$id.__le = function(a, b) return a:key() <= b:key() end
function ob$id:key() return self.v * $M % $K + self.w end
local function mk$id(i) return setmetatable({ v = i * $Q % 97, w = i % 7 }, ob$id) end
local function sorts$id(n)
  local h = 0
  for r = 1, n do
    local t = {}
    for i = 1, 24 + r % 9 do t[i] = mk$id(i + r) end
    table.sort(t, function(a, b)
      if a < b then return true end
      if b < a then return false end
      return a.v < b.v
    end)
    h = (h * 31 + t[1]:key() + 7 * t[#t]:key()) % 1000003
    table.sort(t, function(a, b) return a <= b end)
    h = (h * 31 + t[2]:key()) % 1000003
  end
  return h
end
run("bja$id", sorts$id, $N)
]==], { id = id, N = N(60, 200), M = rnd(1, 50), Q = rnd(1, 90), K = rnd(5, 40) })
end
BJT[#BJT + 1] = function(id)
  return tpl([==[
local function cb$id(w)
  if #w > $L then return w:upper() elseif #w == $L then return #w elseif #w == 1 then return false end
  return nil
end
local function gs$id(n)
  local h = 0
  local text = ("ab cde f ghij k lmnop qr stuvwx y zz "):rep(3)
  for i = 1, n do
    local r, c = text:gsub("%a+", cb$id)
    h = (h * 31 + #r + c) % 1000003
    local r2 = text:gsub("%a+", { ab = "AB", f = 3, k = false })
    h = (h + #r2 + i) % 1000003
    for w in text:gmatch("%a+") do h = (h + #w) % 1000003 end
  end
  return h
end
run("bjb$id", gs$id, $N)
]==], { id = id, N = N(100, 400), L = rnd(2, 4) })
end
BJT[#BJT + 1] = function(id)
  return tpl([==[
local function pf$id(a, b)
  if a % $M == 0 then error("e" .. a) end
  if a % $M == 1 then error({ code = a }) end
  if a % $M == 2 then return nil end
  return a + b, a - b, tostring(a)
end
local function pc$id(n)
  local h = 0
  for i = 1, n do
    local ok, x, y, z = pcall(pf$id, i, 3)
    h = (h * 31 + (ok and 1 or 0) + (type(x) == "number" and x or #tostring(type(x) == "table" and x.code or x)) + (y or 0) + #(z or "")) % 1000003
    local ok2, e = xpcall(pf$id, function(m) return type(m) == "string" and #m or -1 end, i, "x")
    h = (h * 31 + (ok2 and 0 or e)) % 1000003
    if i % 50 == 0 then pf$id = (i % 100 == 0) and function(a, b) return a * 2, b end or pf$id end
  end
  return h
end
run("bjc$id", pc$id, $N)
]==], { id = id, N = N(150, 500), M = rnd(3, 9) })
end
BJT[#BJT + 1] = function(id)
  return tpl([==[
local function co$id(n)
  local h = 0
  for r = 1, n do
    local c = coroutine.wrap(function(a)
      local t = { a, a * 2, a + 3, 1 }
      table.sort(t, function(x, y) return x > y end)
      local ok, e = pcall(table.sort, t, function(x, y) coroutine.yield(1) return x < y end)
      coroutine.yield(t[1] + (ok and 1 or 0))
      return #tostring(e)
    end)
    local v1 = c(r)
    h = (h * 31 + v1) % 1000003
    local ok, v2 = pcall(c)
    h = (h * 31 + (ok and v2 or 0)) % 1000003
  end
  return h
end
run("bjd$id", co$id, $N)
]==], { id = id, N = N(40, 120) })
end
-- BUNDLE-H (JITDIFF_TSET=bh draws only these; not part of TT / T): library calls on trace (the `libcall` limit): n-ary `string.char`, `math.random()` (seeded),
-- `lower` / `upper` / `reverse` / `rep` over strings of both kinds, `type` with a callee that changes, string compares; and traversals (the `tnextinl` limit)
-- of tables that are modified under the loop (nil assigned to or values overwritten in existing fields).
local BHT = {}
BHT[#BHT + 1] = function(id)
  return tpl([==[
local function ch$id(n, bad)
  local h = 0
  for i = 1, n do
    local s = string.char((i * $M) % 256, (i * 7) % 256, (i + $Q) % 256)
    local t = string.char((i * 3) % 256, i % 251)
    h = (h * 31 + s:byte(1) + 2 * s:byte(2) + 3 * s:byte(3) + #t + t:byte(2)) % 1000003
    if bad and i == n // 2 then h = h + #tostring(select(2, pcall(string.char, 65, 256, 1))) end
  end
  return h
end
run("bha$id", ch$id, $N)
run("bhb$id", ch$id, $N, true)
]==], { id = id, N = N(300, 1200), M = rnd(1, 99991), Q = rnd(0, 300) })
end
BHT[#BHT + 1] = function(id)
  return tpl([==[
local function rf$id(n)
  math.randomseed($S, $T)
  local acc, h = 0, 0
  for i = 1, n do
    local x = math.random()
    acc = acc + x
    h = (h * 31 + math.floor(x * 1000) + math.random($K) + math.random(3, $K + 3)) % 1000003
  end
  return h, string.format("%.12g", acc)
end
local function rs$id(n)
  local real = math.random
  local acc = 0
  for i = 1, n do
    if i == n // 2 then math.random = function() return 0.25 end end
    acc = acc + math.random()
  end
  math.random = real
  return string.format("%.12g", acc)
end
run("bhc$id", rf$id, $N)
run("bhd$id", rs$id, $N)
]==], { id = id, N = N(300, 1200), S = rnd(1, 100000), T = rnd(0, 9), K = rnd(2, 1000) })
end
BHT[#BHT + 1] = function(id)
  return tpl([==[
local words$id = { "Hello", "WORLD", "", "mIxEd\0Zz", "\200\201aB", ("Long String "):rep($R), ("x"):rep(41), ("Y"):rep(40) }
local function lu$id(n)
  local h = 0
  for i = 1, n do
    local w = words$id[(i * $M) % #words$id + 1]
    local a, b, c = w:lower(), w:upper(), w:reverse()
    h = (h * 31 + #a + (a:byte(-1) or 0) + (b:byte(1) or 0) + (c:byte(1) or 0)) % 1000003
    local r = w:rep(i % $K, i % 2 == 0 and "," or "")
    h = (h + #r + (r:byte(-1) or 0)) % 1000003
  end
  return h
end
run("bhe$id", lu$id, $N)
]==], { id = id, N = N(300, 1200), M = rnd(1, 9), R = rnd(2, 6), K = rnd(2, 12) })
end
BHT[#BHT + 1] = function(id)
  return tpl([==[
local tv$id = { 1, 1.5, "s", true, {}, print, function() end, coroutine.create(function() end), nil, io.stdout }
local sv$id = { "a", "b", "ab", "", "a\0b", ("p"):rep(50), ("p"):rep(50) .. "q", "\255" }
local realtype$id = type
local function ty$id(n, swap)
  local h = 0
  for i = 1, n do
    if swap and i == n // 2 then type = string.gmatch(("a"):rep(500), "a") end
    local v = tv$id[(i * $M) % 10 + 1]
    h = (h * 31 + #type(v)) % 1000003
    local a, b = sv$id[(i * $M) % 8 + 1], sv$id[(i * 5) % 8 + 1]
    if a < b then h = h + 1 end
    if a <= b then h = h + 2 end
    if a > b then h = h + 4 end
  end
  type = realtype$id
  return h
end
run("bhf$id", ty$id, $N)
run("bhg$id", ty$id, $N, true)
]==], { id = id, N = N(300, 1200), M = rnd(1, 9) })
end
BHT[#BHT + 1] = function(id)
  return tpl([==[
local function tr$id(n)
  local h, t = 0, {}
  for j = 1, $L do t[j] = j t["k" .. j] = j * 2 end
  for i = 1, n do
    local c = 0
    for k, v in pairs(t) do
      c = c + v
    end
    for k, v in pairs(t) do
      if v % $K == i % $K then t[k] = nil elseif i % 3 == 0 then t[k] = v + 1 end
    end
    if i % 5 == 0 then for j = 1, $L do if t[j] == nil then t[j] = j end t["k" .. j] = j * 2 end end
    local k, v = next(t)
    while k ~= nil do c = c + 1 k, v = next(t, k) end
    h = (h * 31 + c) % 1000003
  end
  return h
end
run("bhh$id", tr$id, $N)
]==], { id = id, N = N(100, 400), L = rnd(1, 40), Q = rnd(2, 9), K = rnd(2, 7) })
end

-- BUNDLE-K (JITDIFF_TSET=bk draws only these; not part of TT / T): native self-recursion with lazy CallInfo frames (the `lazyci` limit): fib / tribonacci /
-- ackermann shaped recursion with changing argument types, and observers at depth: traceback, getinfo, getlocal, error + pcall / xpcall, count hook, collectgarbage,
-- coroutines that yield from a leaf, pcall inside the recursion, stack overflow after warm-up, recursion entered from C (sort comparators, gsub callbacks).
local BKT = {}
BKT[#BKT + 1] = function(id)
  return tpl([==[
local fb$id
local cnt$id = 0
local function f$id(n)
  if n < 2 then
    cnt$id = cnt$id + 1
    if cnt$id % $P == 0 then local x = debug.traceback("t", 1); H = (H * 31 + #x) % 1000003 end
    return n
  end
  return f$id(n - 1) + f$id(n - 2)
end
local function bk$id(n)
  local h = 0
  for i = 1, n do h = (h * 31 + f$id($A + i % $B)) % 1000003 end
  return h
end
run("bka$id", bk$id, $N)
]==], { id = id, N = N(40, 160), A = rnd(5, 9), B = rnd(2, 6), P = rnd(7, 211) })
end
BKT[#BKT + 1] = function(id)
  return tpl([==[
local function g$id(n, t)
  if n == 0 then return #t end
  local a = { n }
  local x = g$id(n - 1, { a, t })
  local y = g$id(n - 1, { a })
  return x + y + a[1]
end
local cg$id = 0
local function gc$id(n)
  local h = 0
  for i = 1, n do
    h = (h * 31 + g$id($D + i % 3, {})) % 1000003
    cg$id = cg$id + 1
    if cg$id % $P == 0 then collectgarbage() elseif cg$id % 5 == 0 then collectgarbage("step", 0) end
  end
  return h
end
run("bkb$id", gc$id, $N)
]==], { id = id, N = N(20, 90), D = rnd(4, 7), P = rnd(3, 9) })
end
BKT[#BKT + 1] = function(id)
  return tpl([==[
local function e$id(n, k)
  if n < 2 then
    if k == 1 and n == 0 then error("leaf" .. n) elseif k == 2 and n == 1 then error({ c = n }) end
    return n
  end
  return e$id(n - 1, k) + e$id(n - 2, k)
end
local function er$id(n)
  local h = 0
  for i = 1, n do
    local ok, m = pcall(e$id, $A + i % 4, i % 3)
    h = (h * 31 + (ok and m or (type(m) == "table" and m.c + 1 or #m))) % 1000003
    local ok2, tb = xpcall(e$id, function(m) return #debug.traceback(tostring(m), 1) end, $A + i % 3, 1)
    h = (h * 31 + (ok2 and 0 or tb)) % 1000003
  end
  return h
end
run("bkc$id", er$id, $N)
]==], { id = id, N = N(30, 120), A = rnd(4, 8) })
end
BKT[#BKT + 1] = function(id)
  return tpl([==[
local function ack$id(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack$id(m - 1, 1) end
  return ack$id(m - 1, ack$id(m, n - 1))
end
local function tri$id(n) if n < 3 then return 1 end return tri$id(n - 1) + tri$id(n - 2) + tri$id(n - 3) end
local function ak$id(n)
  local h = 0
  for i = 1, n do
    h = (h * 31 + ack$id(2, i % 4) + tri$id($A + i % 5)) % 1000003
    if i % $P == 0 then h = (h + ack$id(1, 2.0) + tri$id(6.0)) % 1000003 end
  end
  return h
end
run("bkd$id", ak$id, $N)
]==], { id = id, N = N(40, 160), A = rnd(5, 9), P = rnd(5, 30) })
end
BKT[#BKT + 1] = function(id)
  return tpl([==[
local cy$id = 0
local function y$id(n)
  if n < 2 then
    cy$id = cy$id + 1
    if cy$id % $P == 0 then coroutine.yield(n) end
    return n
  end
  return y$id(n - 1) + y$id(n - 2)
end
local function co$id(n)
  local h = 0
  for r = 1, n do
    local c = coroutine.wrap(function() return y$id($A + r % 3) end)
    local steps = 0
    local v
    repeat v = c(); steps = steps + 1 until steps > 60 or v > 5000
    h = (h * 31 + steps + (v or 0)) % 1000003
  end
  return h
end
run("bke$id", co$id, $N)
]==], { id = id, N = N(10, 50), A = rnd(8, 11), P = rnd(3, 17) })
end
BKT[#BKT + 1] = function(id)
  return tpl([==[
local function d$id(n) return 1 + d$id(n + 1) end
local function s$id(n) if n == 0 then return 0 end return 1 + s$id(n - 1) end
local function ov$id(n)
  local h = 0
  for i = 1, n do
    h = (h + s$id(20 + i % 7)) % 1000003
    if i % $P == 0 then
      local ok, m = pcall(d$id, 1)
      h = (h * 31 + (ok and 1 or 0) + (#tostring(m) > 10 and 1 or 0)) % 1000003
    end
  end
  return h
end
run("bkf$id", ov$id, $N)
]==], { id = id, N = N(20, 60), P = rnd(7, 19) })
end
BKT[#BKT + 1] = function(id)
  return tpl([==[
local function h$id(n, k)
  if n < 2 then return n + k end
  local ok, v = pcall(h$id, n - 1, k)
  if not ok then v = -1 end
  return v + h$id(n - 2, k)
end
local function srt$id(a, b) return h$id(a % 9, 1) < h$id(b % 9, 2) or (h$id(a % 9, 1) == h$id(b % 9, 2) and a < b) end
local function sc$id(n)
  local h = 0
  for r = 1, n do
    local t = {}
    for i = 1, 12 + r % 7 do t[i] = (i * $Q + r) % 23 end
    table.sort(t, srt$id)
    h = (h * 31 + t[1] + 5 * t[#t]) % 1000003
    local s = (("abcdef"):gsub(".", function(c) return h$id(#c + $A, 0) end))
    h = (h + #s) % 1000003
  end
  return h
end
run("bkg$id", sc$id, $N)
]==], { id = id, N = N(10, 40), Q = rnd(3, 17), A = rnd(5, 9) })
end
BKT[#BKT + 1] = function(id)
  return tpl([==[
local hk$id = 0
local lv$id
local function l$id(n)
  if n < 2 then
    hk$id = hk$id + 1
    if hk$id % $P == 0 then
      local i, t = 1, {}
      while true do
        local ok, name, v = pcall(debug.getlocal, i + 1, 1)
        if not ok or not name or name:sub(1, 1) == "(" then break end
        t[#t + 1] = tostring(v)
        i = i + 1
      end
      lv$id = (lv$id or 0) + #t
    end
    return n
  end
  return l$id(n - 1) + l$id(n - 2)
end
local function lc$id(n)
  local h = 0
  for i = 1, n do h = (h * 31 + l$id($A + i % 3)) % 1000003 end
  return h + (lv$id or 0)
end
run("bkh$id", lc$id, $N)
]==], { id = id, N = N(20, 90), A = rnd(6, 10), P = rnd(5, 97) })
end
BKT[#BKT + 1] = function(id)
  return tpl([==[
local ev$id, od$id
function ev$id(n) if n == 0 then return true end return od$id(n - 1) end
function od$id(n) if n == 0 then return false end return ev$id(n - 1) end
local function va$id(n, ...) if n == 0 then return select("#", ...) end return va$id(n - 1, n, ...) end
local function tw$id(n) if n < 2 then return n, n + 1 end local a = tw$id(n - 1) local b = tw$id(n - 2) return a + b, a end
local function mu$id(n)
  local h = 0
  for i = 1, n do
    h = (h * 31 + (ev$id(i % 40) and 1 or 2) + va$id(i % 9) + tw$id($A + i % 5)) % 1000003
  end
  return h
end
run("bki$id", mu$id, $N)
]==], { id = id, N = N(40, 160), A = rnd(5, 9) })
end

-- BUNDLE-L (JITDIFF_TSET=bl draws only these; not part of TT / T): array loops (the `aidx` and `bhoist` limits): `a[k + c]` with several offsets over counted
-- loops that run past the array part, shrink / grow the array (insert, remove, a store past the end, `table.move`), alias it through an upvalue, put an
-- `__index` / `__newindex` metatable on it, hold floats / integers / nil / strings mixed, nest two and three levels deep (matrix shape), start at 0 / negative /
-- near the integer limits, count down or step by 2 (not hoisted), error or yield in the middle (getlocal / traceback look at the temporaries).
local BLT = {}
BLT[#BLT + 1] = function(id)
  return tpl([==[
local function bl$id(n)
  local a, b = {}, {}
  for i = 1, $L do a[i] = (i * 3) % 11 + 0.5; b[i] = i % 7 end
  local h = 0
  for r = 1, n do
    local s = 0.0
    for k = 1, $LIM do s = s + (a[k] or 1) * (b[k + $C] or 2) end
    h = (h * 31 + math.floor(s)) % 1000003
    if r % $P == 0 then a[#a + 1] = r; b[#b + 1] = r % 5 end
  end
  return h
end
run("bla$id", bl$id, $N)
]==], { id = id, N = N(30, 120), L = rnd(8, 40), LIM = rnd(4, 60), C = rnd(0, 3), P = rnd(3, 9) })
end
BLT[#BLT + 1] = function(id)
  return tpl([==[
local function bl$id(n)
  local m = {}
  for i = 1, $R do local row = {} for j = 1, $R do row[j] = (i * j + $C) % 9 end m[i] = row end
  local h = 0
  for r = 1, n do
    local t = 0
    for i = 1, $R do
      local mi = m[i]
      for j = 1, $R do
        local s = 0
        for k = 1, $R do s = s + mi[k] * m[k][j] end
        t = t + s
      end
    end
    h = (h * 31 + t) % 1000003
    if r % $P == 0 then m[$R][$R] = r % 5 end
  end
  return h
end
run("blb$id", bl$id, $N)
]==], { id = id, N = N(6, 20), R = rnd(3, 14), C = rnd(0, 5), P = rnd(2, 5) })
end
BLT[#BLT + 1] = function(id)
  return tpl([==[
local function bl$id(n)
  local t = {}
  for i = 1, $L do t[i] = i end
  local h = 0
  for r = 1, n do
    local s = 0
    local ok, err = pcall(function()
      for k = 1, $LIM do
        s = s + t[k] * 2
        if k == $E and r % 4 == 0 then error("e" .. k) end
      end
    end)
    h = (h * 31 + s + (ok and 1 or #tostring(err))) % 1000003
    if r % $P == 0 then t[#t + 1] = r end
    if r % 11 == 0 then t[#t] = nil end
  end
  return h
end
run("blc$id", bl$id, $N)
]==], { id = id, N = N(30, 100), L = rnd(10, 30), LIM = rnd(5, 40), E = rnd(2, 20), P = rnd(3, 8) })
end
BLT[#BLT + 1] = function(id)
  return tpl([==[
local function bl$id(n)
  local t = setmetatable({}, { __index = function(_, k) return k % 3 end })
  for i = 1, $L do t[i] = i % 5 end
  local u = {}
  for i = 1, $L do u[i] = i end
  local h = 0
  for r = 1, n do
    local s = 0
    for k = 1, $LIM do s = s + t[k] + u[k - 1 + 1] + (u[k + 1] or 0) end
    h = (h * 31 + s) % 1000003
    if r % 5 == 0 then table.insert(u, r) end
    if r % 7 == 0 and #u > 2 then table.remove(u) end
    if r % 9 == 0 then t[$L + r] = r end
  end
  return h
end
run("bld$id", bl$id, $N)
]==], { id = id, N = N(30, 100), L = rnd(8, 30), LIM = rnd(5, 45) })
end
BLT[#BLT + 1] = function(id)
  return tpl([==[
local function bl$id(n)
  local t, u = {}, {}
  for i = 1, $L do t[i] = i * 1.5; u[i] = "s" .. i end
  local h = 0
  local co = coroutine.wrap(function()
    for r = 1, n do
      local s = 0.0
      for k = 1, $L do
        s = s + t[k]
        if k % $Y == 0 then coroutine.yield(s) end
      end
    end
    return -1
  end)
  while true do
    local v = co()
    if v == -1 then break end
    h = (h * 31 + math.floor(v)) % 1000003
  end
  local cnt = 0
  for k = $S, $L do local x = u[k] if x then cnt = cnt + #x end end
  return (h + cnt) % 1000003
end
run("ble$id", bl$id, $N)
]==], { id = id, N = N(10, 50), L = rnd(8, 30), Y = rnd(2, 7), S = rnd(-3, 3) })
end
BLT[#BLT + 1] = function(id)
  return tpl([==[
local arr$id = {}
for i = 1, $L do arr$id[i] = i end
local function bl$id(n)
  local h = 0
  for r = 1, n do
    local s = 0
    for k = $A, $B, $STEP do s = s + (arr$id[k] or -1) end
    for k = $B, $A, -$STEP do s = s + (arr$id[k] or -2) end
    for k = math.maxinteger - 3, math.maxinteger do s = s + (arr$id[k] or 1) end
    for k = math.mininteger, math.mininteger + 3 do s = s + (arr$id[k] or 2) end
    h = (h * 31 + s) % 1000003
    if r % 6 == 0 then arr$id = { table.unpack(arr$id, 1, #arr$id - 1) } end
  end
  return h
end
run("blf$id", bl$id, $N)
]==], { id = id, N = N(20, 80), L = rnd(8, 30), A = rnd(-2, 3), B = rnd(5, 40), STEP = rnd(1, 3) })
end

-- BUNDLE-P (JITDIFF_TSET=bp draws only these; not part of TT / T, so the other sets reproduce their old seeds): `f2ifl` (the float-to-integer guard behind
-- `math.floor`: floor / ceil of floats at the edges of the integer range, NaN, +-inf, -0.0, 2^63, -2^63, -2^63 - 1024, results used as table keys and in
-- integer arithmetic), `rearmside` (loops with a class dispatch whose exits get side traces while the root trace is young), and metamethod frames that end in a
-- tail call (`return Vec(...)` through a vararg `__call`, `return setmetatable(...)`, a metamethod that tail-calls another metamethod).
local BPT = {}
BPT[#BPT + 1] = function(id)
  return tpl([==[
local fl$id = { $V1, $V2, $V3, $V4, 0.5, -0.5, 1.5, -2.5, 3.0, -0.0, 2^53, -2^53, 2^62, -2^62, 2^63 - 1024, 2^63, -2^63, -2^63 - 1024 * 3, 1/0, -1/0, 0/0, 1e300, -1e300,
  123456.789, 4.5e15, 9007199254740993.0, math.pi, -math.pi }
local function run$id(n)
  local h, ints, fls = 0, 0, 0
  local tbl = { 10, 20, 30, 40, 50 }
  for i = 1, n do
    local v = fl$id[(i * $A) % #fl$id + 1]
    local f, c = math.floor(v), math.ceil(v)
    if math.type(f) == "integer" then ints = ints + 1; h = h ~ f else fls = fls + 1 end
    if math.type(c) == "integer" then ints = ints + 1; h = h ~ (c + 1) else fls = fls + 1 end
    mix(f); mix(c)
    local k = math.floor(v / 1e18)
    if math.type(k) == "integer" then mix(tbl[k % 5 + 1]) end
    mix(math.floor(i / $D) * $D + math.floor(-i / $D))
  end
  mix(h); mix(ints); mix(fls)
end
run("bpa$id", run$id, $N)
-- the same value many times (the trace specialises on it), then a change of type at the end of the loop
local function same$id(v, n)
  local s, k = 0, 0
  for i = 1, n do
    local f = math.floor(v)
    if math.type(f) == "integer" then s = s + f else k = k + 1 end
  end
  return s, k
end
for j = 1, #fl$id do local a, b = same$id(fl$id[j], $M); mix(a); mix(b) end
]==], { id = id, N = N(300, 900), M = N(150, 400), A = pick{ 1, 3, 5, 7 }, D = pick{ 2, 3, 7, 10 }, V1 = pick{ "1e15", "-1e15", "2^31", "-2^31" },
        V2 = pick{ "0.25", "-0.75", "1e-300", "5e-324" }, V3 = pick{ "2^52", "-2^52 - 1", "2^52 + 0.5" }, V4 = pick{ "9.2e18", "-9.2e18", "9223372036854774784.0" } })
end
BPT[#BPT + 1] = function(id)
  return tpl([==[
local cls$id = {}
local ks$id = {}
for i = 1, $K do
  local k = {}
  k.__index = k
  k.step = function(self, d) self.x = self.x + d * i; if i % 3 == 0 then self.n = (self.n or 0) + 1 end; return self.x + (self.n or 0) end
  ks$id[i] = k
end
local objs$id = {}
for i = 1, $L do objs$id[i] = setmetatable({ x = i }, ks$id[i % $K + 1]) end
local function run$id(n)
  local acc = 0
  for step = 1, n do
    for i = 1, #objs$id do acc = acc + objs$id[i]:step(1) end
    if step % $P == 0 then objs$id[step % #objs$id + 1] = setmetatable({ x = step }, ks$id[step % $K + 1]) end
  end
  mix(acc)
end
run("bpb$id", run$id, $N)
]==], { id = id, N = N(150, 350), K = rnd(2, 6), L = rnd(8, 50), P = rnd(7, 60) })
end
BPT[#BPT + 1] = function(id)
  return tpl([==[
local Obj$id = {}
Obj$id.__index = Obj$id
function Obj$id:__call(...)
  local o = setmetatable({}, self)
  o:new(...)
  return o
end
local V$id = setmetatable({}, Obj$id)
V$id.__index = V$id
function V$id:new(x, y) self.x = x or 0; self.y = y or 0 end
function V$id.__add(a, b) return V$id(a.x + b.x, a.y + b.y) end
function V$id.__sub(a, b) local r = V$id(a.x - b.x, a.y - b.y); return r end
function V$id.__mul(a, b)
  if type(a) == "number" then return V$id(a * b.x, a * b.y) elseif type(b) == "number" then return V$id(a.x * b, a.y * b) end
  return a.x * b.x + a.y * b.y
end
function V$id.__div(a, b) return V$id(a.x / b, a.y / b) end
function V$id.__unm(a) return V$id(-a.x, -a.y) end
function V$id.__eq(a, b) return a.x == b.x and a.y == b.y end
function V$id.__lt(a, b) return a.x < b.x or (a.x == b.x and a.y < b.y) end
function V$id.__le(a, b) return a.x <= b.x and a.y <= b.y end
function V$id.__len(a) return 2 end
function V$id.__concat(a, b) return tostring(type(a) == "table" and a.x or a) .. "|" .. tostring(type(b) == "table" and b.y or b) end
function V$id:len2() return self.x * self.x + self.y * self.y end
function V$id:norm() local l = math.sqrt(self:len2()); if l > 0 then return self / l end return V$id(0, 0) end
local function run$id(n)
  local p, v = V$id(1, $X), V$id($Y, 2)
  local acc, cnt = 0, 0
  for i = 1, n do
    p = (p + v * 0.01) * 0.999 - (-v) / 100
    local d = p - v
    acc = acc + d:len2() % 1000 + #p
    if i % $M == 0 then p = V$id(i % 7, 1); v = V$id(1, i % 3); mix(p .. v) end
    if p < v then cnt = cnt + 1 end
    if p <= v and p == p:norm() * 0 + p then cnt = cnt + 2 end
    if i == n // 2 then V$id.__sub = function(a, b) return V$id(0, 0) end end
    if i == (n * 3) // 4 then V$id.__add = function(a, b) return a end end
  end
  mix(math.floor(acc)); mix(cnt); mix(tostring(p.x - p.x))
end
run("bpc$id", run$id, $N)
]==], { id = id, N = N(150, 500), X = pick{ 0, 1, 2 }, Y = pick{ 1, 3, 5 }, M = rnd(11, 60) })
end

-- BUNDLE-Q (JITDIFF_TSET=bq draws only these; not part of TT / T, so the other sets reproduce their old seeds): `t1q` bit 0 (tier-1 `==` / `~=` of tables, userdata and
-- functions without a real `__eq` call: the same object, distinct objects with no / absent / present `__eq`, `__eq` returning true / false / non-booleans, `__eq` on one
-- operand only, `__eq` added and removed after the absence was cached) and bit 1 (`%` with a float operand: signs, zeros, inf, nan, huge values, int / float mixes).
local BQT = {}
BQT[#BQT + 1] = function(id)
  return tpl([==[
local mt0$id = {}
mt0$id.__index = mt0$id
local mtt$id = { __eq = function(a, b) return $R1 end }
local mtf$id = { __eq = function(a, b) return $R2 end }
local mtn$id = {}
local ob$id = {}
for i = 1, $K do ob$id[#ob$id + 1] = {} end
for i = 1, $K do ob$id[#ob$id + 1] = setmetatable({}, mt0$id) end
for i = 1, $K do ob$id[#ob$id + 1] = setmetatable({}, i % 2 == 0 and mtt$id or mtf$id) end
ob$id[#ob$id + 1] = setmetatable({}, mtn$id)
ob$id[#ob$id + 1] = setmetatable({}, mtn$id)
ob$id[#ob$id + 1] = print
ob$id[#ob$id + 1] = type
ob$id[#ob$id + 1] = io.stdout
ob$id[#ob$id + 1] = io.stderr
ob$id[#ob$id + 1] = coroutine.create(print)
ob$id[#ob$id + 1] = function() end
ob$id[#ob$id + 1] = 3
ob$id[#ob$id + 1] = 3.0
ob$id[#ob$id + 1] = "s"
ob$id[#ob$id + 1] = string.rep("q", 50) .. $I
ob$id[#ob$id + 1] = string.rep("q", 50) .. $I
ob$id[#ob$id + 1] = false
local function run$id(n)
  local m, h, c = #ob$id, 0, 0
  for i = 1, n do
    local a, b = ob$id[(i * $A) % m + 1], ob$id[(i * $B + i // $D) % m + 1]
    if a == b then c = c + 1 end
    if a ~= b then c = c + 3 end
    if a == a then c = c + 5 end
    local ok, r = pcall(function() return a == b end)
    h = h + (r and 1 or 0)
    if i == n // 3 then mtn$id.__eq = function() return true end end
    if i == n // 2 then mtn$id.__eq = nil end
    if i == (n * 2) // 3 then mt0$id.__eq = function(x, y) return rawequal(getmetatable(x), getmetatable(y)) end end
    if i == (n * 5) // 6 then mt0$id.__eq = nil end
  end
  mix(c); mix(h)
end
run("bqa$id", run$id, $N)
]==], { id = id, N = N(300, 1200), K = rnd(2, 4), A = pick{ 1, 3, 5, 7 }, B = pick{ 2, 5, 11 }, D = pick{ 3, 7, 13 }, I = pick{ "1", "2" },
        R1 = pick{ "true", "1", "'x'", "nil" }, R2 = pick{ "false", "nil", "0", "a == b" } })
end
BQT[#BQT + 1] = function(id)
  return tpl([==[
local fv$id = { 0, 1, -1, 2, -2, 7, -7, 255, math.maxinteger, math.mininteger, 0.0, -0.0, 0.5, -0.5, 1.5, -1.5, 2.5, -2.5, 5.5, -5.5, 1e15, -1e15,
  2^53, -2^53, 2^63, -2^63, 1e308, -1e308, 1e-308, -1e-308, 5e-324, -5e-324, 1/0, -1/0, 0/0, 3.0, -3.0, $V1, $V2 }
local function fm$id(x) if x == 0 then return 1 / x > 0 and "+0" or "-0" end return string.format("%.17g", x) end
local function run$id(n)
  local m = #fv$id
  for i = 1, n do
    local a, b = fv$id[(i * $A) % m + 1], fv$id[(i * $B + i // $D) % m + 1]
    local ok, r = pcall(function() return a % b end)
    mix(ok and (math.type(r) .. fm$id(r)) or "err")
    local x = a % $K1
    local y = a % $K2
    local z = a % -$K3
    mix(fm$id(x)); mix(fm$id(y)); mix(fm$id(z))
    local ok2, w = pcall(function() return b % 0 end)
    mix(ok2 and fm$id(w) or "err0")
    mix(fm$id(a % 0.0)); mix(fm$id(a % (1 / 0)))
  end
end
run("bqb$id", run$id, $N)
]==], { id = id, N = N(300, 1500), A = pick{ 1, 3, 5, 7 }, B = pick{ 2, 5, 11 }, D = pick{ 3, 7, 13 }, V1 = pick{ "2.5", "6.02e23", "1e-5" },
        V2 = pick{ "4.0", "-1e300", "123456789.123" }, K1 = pick{ "2.5", "3.0", "0.1", "1e308", "256.0" }, K2 = pick{ "7", "256", "1e15", "0.5" }, K3 = pick{ "2.0", "1e-300", "3.5", "5" } })
end
BQT[#BQT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local acc, k = 0.0, 0
  local prev
  local node = { id = 0 }
  local goal = { id = n }
  local mtn = {}
  for i = 1, n do
    local cur = (i % $M == 0) and goal or { id = i }
    if cur == goal then k = k + 1 end
    if prev ~= nil and cur ~= prev then k = k + 2 end
    if cur == cur and prev == prev then k = k + 4 end
    prev = cur
    local f = (i * 0.5) % $P
    acc = acc + f + (i + 0.25) % 3 + (-i) % 2.5
    if (i * 0.5) % 2 == 0 then k = k + 1 end
    if i == n // 2 then setmetatable(goal, { __eq = function() return true end }) end
  end
  mix(math.floor(acc * 1000)); mix(k)
end
run("bqc$id", run$id, $N)
]==], { id = id, N = N(300, 1500), M = rnd(3, 20), P = pick{ 2, 3, 7 } })
end

-- BUNDLE-V (JITDIFF_TSET=bv draws only these; not part of TT / T): `t1q` bits 4 / 5 / 6 (arithmetic / bitwise / unary metamethod handlers run as frames of tier 1,
-- tail calls and general returns inside metamethod frames) and `upbar` (stores of collectable values into upvalue cells).
local BVT = {}
BVT[#BVT + 1] = function(id)
  return tpl([==[
local V$id = {}
local calls$id = 0
local function new$id(x, y) return setmetatable({ x = x, y = y }, V$id) end
local function tl$id(a, b) return new$id(a.x + b.x, a.y + b.y) end
local function tv$id(...) return new$id(select("#", ...), $VX) end
local call$id = setmetatable({}, { __call = function(self, a, b) calls$id = calls$id + 1; return new$id(a.x - b.x, $CK) end })
V$id.__add = $ADD
V$id.__sub = function(a, b) return tv$id(a, b, $VA) end
V$id.__mul = function(a, b) if type(b) == "number" then return new$id(a.x * b, a.y * b) end return call$id(a, b) end
V$id.__unm = function(a) return $UNM end
V$id.__div = function(a, b) return $DIV end
V$id.__mod = function(a, b) return select($SEL, a.x, b, 7) end
V$id.__concat = function(a, b) return "c" end
V$id.__len = function(a) return a.x end
V$id.__lt = function(a, b) return a.x < b.x end
V$id.__le = function(a, b) return tl$id(a, b).x <= 100 end
local function run$id(n)
  local a, b = new$id(1, 2), new$id(3, 4)
  local s = 0
  for i = 1, n do
    local c = a + b
    local d = a - b
    local e = c * 2
    local f = c * d
    local g = -c
    local h = a / 2
    local k = a % 3
    s = s + c.x + d.x + e.y + f.x + g.y + h + (type(k) == "number" and k or 0) + #(a .. b) + #a
    if a < b then s = s + 1 end
    if a <= b then s = s + 2 end
    if i == n // 3 then V$id.__add = function(p, q) return $ADD2 end end
    if i == (n * 2) // 3 then V$id.__unm = nil end
    if i % 61 == 0 then a = new$id(i, i + 1) end
  end
  mix(s); mix(calls$id)
end
run("bva$id", run$id, $N)
]==], { id = id, N = N(200, 900), VX = pick{ "0", "1", "select('#', 1, 2)" }, CK = pick{ "b.y", "7", "a.y * 2" },
        ADD = pick{ "tl" .. id, "function(a, b) return tl" .. id .. "(a, b) end", "function(a, b) return new" .. id .. "(a.x + b.x, a.y + b.y) end",
                    "function(a, b) return math.max(a.x, b.x) end", "function(a, b) return tostring(a.x) end" },
        ADD2 = pick{ "new" .. id .. "(-1, -1)", "p", "nil", "q.x" }, VA = pick{ "1", "1, 2, 3", "nil" },
        UNM = pick{ "new" .. id .. "(-a.x, -a.y)", "setmetatable({ x = -a.x, y = 0 }, V" .. id .. ")", "rawequal(a, a)", "a.x, a.y" },
        DIV = pick{ "new" .. id .. "(a.x / b, a.y / b)", "tl" .. id .. "(a, a)", "(function(...) return ... end)(a.x, b)" }, SEL = pick{ "1", "2", "3" } })
end
BVT[#BVT + 1] = function(id)
  return tpl([==[
local W$id = {}
local function boom$id(x) error("boom" .. tostring(x), $LV) end
W$id.__add = function(a, b) if a.err then return boom$id(a.err) end return a.v + b.v end
W$id.__sub = function(a, b) if a.err then error({ code = a.err }) end local r = coroutine.isyieldable() and coroutine.yield($YV) or 1 return r end
W$id.__unm = function(a) return coroutine.isyieldable() and coroutine.yield(a.v) or a.v end
W$id.__mul = function(a, b) $DBG return 5 end
local function mk$id(v, e) return setmetatable({ v = v, err = e }, W$id) end
local function body$id(n)
  local s = 0
  for i = 1, n do
    local a, b = mk$id(i, i % $E == 0 and i or nil), mk$id(2)
    local ok, r = pcall(function() return a + b end)
    s = s + (ok and r or #tostring(r))
    ok, r = pcall(function() return a - b end)
    s = s + (ok and r or (type(r) == "table" and r.code or 1))
    s = s + (-b) + (a * b)
  end
  return s
end
local function run$id(n)
  local co = coroutine.wrap(function() return body$id(n) end)
  local r = co()
  local steps = 0
  while type(r) == "number" and steps < 4 * n do steps = steps + 1; r = co(steps % 4) end
  mix(r); mix(steps)
  mix(body$id(n // 2))
end
run("bvb$id", run$id, $N)
]==], { id = id, N = N(80, 400), LV = pick{ "1", "2", "0" }, YV = pick{ "7", "'y'", "nil" }, E = rnd(3, 11),
        DBG = pick{ "", "local i = debug.getinfo(1, 'l'); debug.traceback('x', 1)", "debug.sethook(function() end, 'l'); debug.sethook()", "local n = debug.getlocal(1, 1)" } })
end
BVT[#BVT + 1] = function(id)
  return tpl([==[
local head$id, tail$id, cur$id, n$id = nil, nil, nil, 0
local function push$id(x)
  x.next = head$id
  if head$id then head$id.prev = x end
  head$id = x
  if not tail$id then tail$id = x end
  n$id = n$id + 1
end
local function drop$id()
  local t = tail$id
  if not t then return end
  if t.prev then t.prev.next = nil end
  tail$id = t.prev
  if not tail$id then head$id = nil end
  n$id = n$id - 1
end
local function set$id(v) cur$id = v end
local function run$id(n)
  local s = 0
  for i = 1, n do
    push$id({ k = i, v = { i } })
    if n$id > $CAP then drop$id() end
    local m = i % 6
    if m == 0 then set$id({ i }) elseif m == 1 then set$id("k" .. i) elseif m == 2 then set$id(nil) elseif m == 3 then set$id(false)
    elseif m == 4 then set$id(i / 4) else set$id(function() return i end) end
    local t = type(cur$id)
    s = s + (head$id and head$id.k or 0) + (tail$id and tail$id.k or 0) + #t
    if t == "table" then s = s + cur$id[1] elseif t == "string" then s = s + #cur$id elseif t == "function" then s = s + cur$id() end
    if i % $GC == 0 then collectgarbage("step", $ST) end
  end
  mix(s); mix(n$id)
end
run("bvc$id", run$id, $N)
]==], { id = id, N = N(200, 1000), CAP = rnd(2, 9), GC = rnd(2, 9), ST = pick{ "0", "1", "10" } })
end
-- BUNDLE-S (JITDIFF_TSET=bs draws only these; not part of TT / T): `amt` (array reads / stores of tables with a metatable: holes, `__index` / `__newindex`
-- added, removed and replaced in the loop, `setmetatable(t, nil)`, zero / negative / float keys) and `t1q` bits 2 / 3 (tier-1 table reads / stores whose `__index` /
-- `__newindex` is a Lua function: all instruction forms, chains of tables ending in a function, handlers that return none / many values, error, yield, rawset,
-- replace the metamethod or the metatable of the table they serve).
local BST = {}
BST[#BST + 1] = function(id)
  return tpl([==[
local mt$id = { $MT }
local t$id = setmetatable({}, mt$id)
for i = 1, $L do t$id[i] = i * $Q end
local hole$id = $H
t$id[hole$id] = nil
local function run$id(n)
  local s, c = 0, 0
  for i = 1, n do
    local k = i % $M - $O
    local v = t$id[k]
    if type(v) == "number" then s = s + v elseif v ~= nil then c = c + #tostring(v) end
    t$id[i % $L + 1] = i
    if i == n // 5 then mt$id.__index = function(_, kk) return -kk end end
    if i == n // 3 then mt$id.__newindex = function(tt, kk, vv) rawset(tt, kk, (vv or 0) * 2) end end
    if i == n // 2 then t$id[hole$id] = nil; t$id[$L + 1] = nil end
    if i == (n * 2) // 3 then mt$id.__index = nil end
    if i == (n * 3) // 4 then mt$id.__newindex = nil end
    if i == (n * 4) // 5 then setmetatable(t$id, nil) end
    if i == (n * 9) // 10 then setmetatable(t$id, { __index = function() return 1.5 end }) end
  end
  mix(s); mix(c); mix(#t$id)
end
run("bsa$id", run$id, $N)
]==], { id = id, N = N(300, 1500), L = rnd(5, 40), M = rnd(3, 50), O = pick{ 0, 1, 3, 10 }, Q = pick{ 1, 2, 7 }, H = rnd(1, 4),
        MT = pick{ "__tostring = tostring", "__index = function(_, k) return k * 2 end", "__index = { [3] = 'x' }, __newindex = function() end",
                   "__newindex = function(t, k, v) rawset(t, k, v) end" } })
end
BST[#BST + 1] = function(id)
  return tpl([==[
local calls$id = 0
local mt$id = { __index = function(t, k) calls$id = calls$id + 1; return $R end }
local base$id = setmetatable({ a = 1 }, mt$id)
local mid$id = setmetatable({}, { __index = base$id })
local o$id = setmetatable({ x = 10 }, { __index = $CH })
local env$id = setmetatable({}, { __index = function(_, k) calls$id = calls$id + 1; return #k end })
local gl$id = load("local n = ... local s = 0 for i = 1, n do s = s + zz + yyy end return s", "g", "t", env$id)
local function run$id(n)
  local s = 0
  for i = 1, n do
    local v = o$id.x
    local q = o$id.q
    if type(q) == "number" then v = v + q end
    if type(v) == "number" then s = s + v end
    s = s + (type(o$id.w) == "number" and o$id.w or 0) + (type(o$id[i]) == "number" and 1 or 0)
    local ok, m = pcall(function() return o$id:meth() end)
    if ok and type(m) == "number" then s = s + m end
    if i == n // 4 then mt$id.__index = function(t, k) return $R2 end end
    if i == n // 2 then rawset(o$id, "q", 5) end
    if i == (n * 3) // 5 then o$id.q = nil end
    if i == (n * 3) // 4 then setmetatable(o$id, { __index = mid$id }) end
    if i == (n * 9) // 10 then mt$id.__index = nil end
  end
  mix(s); mix(calls$id); mix(gl$id(20))
end
run("bsb$id", run$id, $N)
]==], { id = id, N = N(300, 1200), R = pick{ "k", "1", "nil", "t", "k, 2, 3", "(function() return 4 end)()" }, R2 = pick{ "0", "'s'", "k" },
        CH = pick{ "base" .. id, "mid" .. id } })
end
BST[#BST + 1] = function(id)
  return tpl([==[
local log$id = {}
local mt$id = { __newindex = function(t, k, v) log$id[#log$id + 1] = tostring(k); $BODY end }
local t$id = setmetatable({ 1, 2, 3, a = 1 }, mt$id)
local e$id = setmetatable({}, { __newindex = function(t, k, v) log$id[#log$id + 1] = "g" .. k; rawset(t, k, v) end })
local gl$id = load("local n = ... for i = 1, n do gv = i; gw = (gw or 0) + 1 end return gv", "g", "t", e$id)
local function run$id(n)
  local s = 0
  for i = 1, n do
    local ok = pcall(function()
      t$id.a = i
      t$id.b = i
      t$id[i % 7 + 1] = i
      t$id[i % 5 - 2] = i
      t$id[i + 0.5] = i
      t$id["k" .. i % 4] = i
      t$id.z = i
    end)
    if not ok then s = s + 1 end
    if i == n // 4 then mt$id.__newindex = function(t, k, v) rawset(t, k, v) end end
    if i == n // 2 then rawset(t$id, 2, nil) end
    if i == (n * 3) // 5 then mt$id.__newindex = nil end
    if i == (n * 3) // 4 then mt$id.__newindex = function(t, k, v) error("ro") end end
  end
  mix(s); mix(#log$id); mix(rawlen(t$id)); mix(gl$id(30))
end
run("bsc$id", run$id, $N)
]==], { id = id, N = N(300, 1200), BODY = pick{ "rawset(t, k, v)", "if v % 3 == 0 then rawset(t, k, v) end", "", "local x = v + 1" } })
end
BST[#BST + 1] = function(id)
  return tpl([==[
local t$id = setmetatable({}, { __index = function(_, k) return coroutine.yield(k) end,
                              __newindex = function(t, k, v) coroutine.yield(v); rawset(t, k, v) end })
local function body$id(n)
  local s = 0
  for i = 1, n do
    s = s + t$id.a + t$id[i % 4]
    t$id[i % 3] = i
  end
  return s
end
local function run$id(n)
  local co = coroutine.create(body$id)
  local ok, v = coroutine.resume(co, n)
  local c = 0
  while coroutine.status(co) == "suspended" do
    c = c + 1
    ok, v = coroutine.resume(co, c % $M)
  end
  mix(ok); mix(v); mix(c)
end
run("bsd$id", run$id, $N)
]==], { id = id, N = N(40, 200), M = rnd(2, 9) })
end

-- BUNDLE-T (JITDIFF_TSET=bt draws only these; not part of TT / T): coroutine switch shapes (the fused resume of `coroutine.wrap` / `coroutine.resume`): generators
-- with changing argument / result counts and wanted-result counts at the yield site, bodies with hot loops, yields across `pcall` / `xpcall` / metamethods /
-- iterators, errors and `close` between switches, nested resumes (the C level), large value counts (the stacks grow), garbage coroutines, `yield` as a C metamethod.
local BTT = {}
BTT[#BTT + 1] = function(id)
  return tpl([==[
local function gen$id(n)
  return coroutine.wrap(function(a, b)
    local s = 0
    for i = 1, n do
      s = s + i * (a or 1) + #tostring(b)
      $YIELD
    end
    return s, "end"
  end)
end
local function run$id(n)
  local g = gen$id(n)
  local acc, a, b, c = 0, 1, "x", 0
  while true do
    local r1, r2, r3 = g(a, b)
    c = c + 1
    if r2 == "end" then acc = acc + r1 break end
    acc = acc + (r1 or 0) + (r3 or 0)
    a = (a + c) % 7 + 1
    b = (r2 == nil) and "q" or r2
  end
  mix(acc); mix(c)
end
for k = 1, $M do run("btg$id", run$id, $N) end
]==], { id = id, N = N(30, 300), M = rnd(1, 4),
        YIELD = pick{ "coroutine.yield()", "local x = coroutine.yield(s)", "local x, y, z = coroutine.yield(s, a, i); a = (x or 0) + (z or 0)",
                      "a = coroutine.yield(i, s)", "b = select('#', coroutine.yield(s, s, s))", "local t = {coroutine.yield(i)}; b = #t" } })
end
BTT[#BTT + 1] = function(id)
  return tpl([==[
local co$id = coroutine.create(function(...)
  local n = select('#', ...)
  local tot = 0
  while true do
    local t = table.pack(coroutine.yield(n, ...))
    n = t.n
    for i = 1, t.n do if type(t[i]) == "number" then tot = tot + t[i] end end
    if t[1] == "stop" then return tot, n end
    $EXTRA
  end
end)
local function run$id(n)
  local c = 0
  local acc = 0
  for i = 1, n do
    local k = i % $K
    local args = {}
    for j = 1, k do args[j] = (j % 3 == 0) and "s" or j * i end
    local r = table.pack(coroutine.resume(co$id, table.unpack(args, 1, k)))
    acc = acc + r.n + (type(r[2]) == "number" and r[2] or 0)
    c = c + (coroutine.status(co$id) == "suspended" and 1 or 0)
    if i == n - 1 then local r2 = table.pack(coroutine.resume(co$id, "stop", i)) acc = acc + r2.n + (r2[2] or 0) end
  end
  local r3 = table.pack(coroutine.resume(co$id))
  mix(acc); mix(c); mix(r3.n); mix(tostring(r3[1])); mix(tostring(r3[2])); mix(coroutine.status(co$id))
end
run("btr$id", run$id, $N)
]==], { id = id, N = N(30, 300), K = rnd(2, 40), EXTRA = pick{ "", "if tot > 1e6 then tot = 0 end", "if n > 20 then error('big') end" } })
end
BTT[#BTT + 1] = function(id)
  return tpl([==[
local function body$id(n)
  local s = 0
  for i = 1, n do
    local ok, e = pcall(function()
      local v = coroutine.yield(i)
      if v == $BAD then error("bad" .. i, 0) end
      s = s + (v or 0)
    end)
    if not ok then s = s + #e end
    local ok2, e2 = xpcall(function() local w = coroutine.yield(s) if (w or 0) % 5 == 0 then error({code = i}) end return w end,
                           function(m) return type(m) == "table" and m.code or m end)
    s = s + (tonumber(e2) or 0)
    if i == $STOP then error("stopped") end
  end
  return s
end
local function run$id(n)
  local co = coroutine.create(body$id)
  local ok, v = coroutine.resume(co, n)
  local c, acc = 0, 0
  while coroutine.status(co) == "suspended" do
    c = c + 1
    acc = acc + (v or 0)
    ok, v = coroutine.resume(co, c % 11)
  end
  mix(ok); mix(tostring(v)); mix(c); mix(acc)
  mix(select(2, coroutine.resume(co)))
end
run("btp$id", run$id, $N)
]==], { id = id, N = N(20, 120), BAD = rnd(0, 10), STOP = rnd(5, 100) })
end
BTT[#BTT + 1] = function(id)
  return tpl([==[
local ymt$id = { __index = coroutine.yield, __add = coroutine.yield, __concat = coroutine.yield, __len = coroutine.yield, __call = coroutine.yield, __lt = coroutine.yield }
local o$id = setmetatable({}, ymt$id)
local function body$id(n)
  local s = 0
  for i = 1, n do
    local a = o$id.k
    local b = o$id + i
    local c = o$id .. "x"
    local d = #o$id
    local e = o$id(i)
    local f = o$id < o$id
    s = s + (tonumber(a) or 0) + (tonumber(b) or 0) + #tostring(c) + (tonumber(d) or 0) + (tonumber(e) or 0) + (f and 1 or 0)
  end
  return s
end
local function run$id(n)
  local co = coroutine.wrap(body$id)
  local v = co(n)
  local c, acc = 0, 0
  while c < $LIM do
    c = c + 1
    if type(v) == "number" and c > n * 6 then break end
    acc = acc + c
    v = co(c % 4 == 0 and c or tostring(c % 5))
  end
  mix(c); mix(acc); mix(type(v))
end
run("btm$id", run$id, $N)
]==], { id = id, N = N(5, 40), LIM = rnd(30, 400) })
end
BTT[#BTT + 1] = function(id)
  return tpl([==[
local function chain$id(d, n)
  if d == 0 then
    local g = coroutine.wrap(function() for i = 1, n do coroutine.yield(i, d) end return "done" end)
    local s = 0
    for _ = 1, n + 1 do local v = g() if type(v) == "number" then s = s + v else s = s + #v end end
    return s
  end
  return coroutine.wrap(function() return chain$id(d - 1, n) + d end)()
end
local function deepest$id(d)
  local ok, e = pcall(function() local co = coroutine.wrap(function() return deepest$id(d + 1) end) return co() end)
  if not ok then error(e, 0) end
  return e
end
local function run$id(n)
  local s = 0
  for i = 1, n do s = s + chain$id(i % $D, i % 5 + 1) end
  mix(s)
  local ok, e = pcall(deepest$id, 1)
  mix(ok); mix(type(e) == "string" and #e > 0)
end
run("btn$id", run$id, $N)
]==], { id = id, N = N(5, 40), D = rnd(2, 12) })
end
BTT[#BTT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local big = {}
  for i = 1, $B do big[i] = i end
  local g = coroutine.wrap(function(...)
    local k = select('#', ...)
    while true do
      local t = {coroutine.yield(k, ...)}
      k = #t
    end
  end)
  local s = 0
  for i = 1, n do
    local m = (i * 37) % $B
    local r = table.pack(g(table.unpack(big, 1, m)))
    s = s + r.n + r[1]
    local co = coroutine.create(function() coroutine.yield(table.unpack(big, 1, m)) return m end)
    local ok, a, b = coroutine.resume(co)
    s = s + (b or 0)
    ok, a = coroutine.resume(co)
    s = s + (a or 0)
  end
  mix(s)
end
run("btb$id", run$id, $N)
]==], { id = id, N = N(5, 40), B = pick{ 50, 300, 2500 } })
end
BTT[#BTT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local live, s = {}, 0
  for i = 1, n do
    local co = coroutine.create(function(a)
      local t <close> = setmetatable({}, { __close = function() s = s + 1 end })
      local b = coroutine.yield(a * 2)
      local c = coroutine.yield(b + 1)
      return a + b + (c or 0)
    end)
    local ok, x = coroutine.resume(co, i)
    s = s + x
    if i % $P == 0 then ok, x = coroutine.resume(co, i) s = s + x end
    if i % $P == 1 then live[#live + 1] = co end
    if i % $P == 2 then coroutine.close(co) end
    if i % 40 == 0 then collectgarbage() end
    if #live > 8 then local c = table.remove(live, 1) local _, y = coroutine.resume(c, 3) s = s + (y or 0) coroutine.resume(c, 4) end
  end
  mix(s); mix(#live)
end
run("btc$id", run$id, $N)
]==], { id = id, N = N(30, 300), P = rnd(3, 7) })
end

-- BUNDLE-R (JITDIFF_TSET=br draws only these; not part of TT / T): `table.unpack` / `table.move` of small ranges, `tonumber` of strings that are no numerals,
-- `string.rep` whose result class (short / long) varies, and concatenated string keys (the `libcall` bits 3..8).
local BRT = {}
BRT[#BRT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local acc, cnt = 0, 0
  for i = 1, n do
    local t = {}
    for j = 1, i % $K do t[j] = j * i + $C end
    local a, b, c, d = table.unpack(t)
    acc = acc + (a or 0) + (b or 0) * 3 + (c or 0) * 5 + (d or 0) * 7
    cnt = cnt + select("#", table.unpack(t))
    $EXTRA
  end
  mix(acc); mix(cnt)
end
run("bru$id", run$id, $N)
]==], { id = id, N = N(40, 400), K = rnd(2, 9), C = rnd(0, 5),
        EXTRA = pick{ "", "if i % 17 == 0 then t[#t] = nil end", "if i % 11 == 0 then setmetatable(t, { __len = function() return 2 end }) acc = acc + select('#', table.unpack(t)) end",
                      "if i % 13 == 0 then t[#t + 1] = 'x' end acc = acc + #{ table.unpack(t) }" } })
end
BRT[#BRT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local src = {}
  for j = 1, $S do src[j] = j * 3 end
  local dst, acc = {}, 0
  for i = 1, n do
    local f, e, d = $F, $E, $D
    $PRE
    table.move(src, f, e, d, $DST)
    acc = acc + (dst[d] or 0) + (src[e] or 0) + #dst
  end
  local out = {}
  for j = 1, $S + 4 do out[#out + 1] = tostring(dst[j]) .. "/" .. tostring(src[j]) end
  mix(acc); mix(table.concat(out, ","))
end
run("brm$id", run$id, $N)
]==], { id = id, N = N(40, 400), S = rnd(6, 14), F = rnd(1, 3), E = rnd(3, 7), D = rnd(1, 5),
        PRE = pick{ "", "if i % 29 == 0 then e = e + 1 end", "if i % 23 == 0 then d = d + 2 end", "if i % 31 == 0 then f = f + 1 end" },
        DST = pick{ "dst", "src", "(i % 3 == 0) and src or dst", "nil" } })
end
BRT[#BRT + 1] = function(id)
  return tpl([==[
local forms$id = { "abc", "", "  ", "0x", "1e", "5 5", "12", " 7 ", "0x10", "1e2", ".5", "x1", "9z", "--1", "inf", "nan" }
local function run$id(n)
  local acc, nils, fl = 0, 0, 0
  for i = 1, n do
    local v = tonumber(forms$id[(i * $M) % #forms$id + 1])
    if v then acc = acc + v if math.type(v) == "float" then fl = fl + 1 end else nils = nils + 1 end
    $EXTRA
  end
  mix(acc); mix(nils); mix(fl)
end
run("brt$id", run$id, $N)
]==], { id = id, N = N(40, 600), M = pick{ 1, 3, 5, 7 }, EXTRA = pick{ "", "local w = tonumber('z' .. i) if w then acc = acc + 1000 end", "if tonumber('q') == nil then nils = nils + 1 end" } })
end
BRT[#BRT + 1] = function(id)
  return tpl([==[
local units$id = { "a", "ab", "xyz", "", ("q"):rep(7) }
local seps$id = { "", ",", ", ", "--" }
local function run$id(n)
  local acc, ls = 0, 0
  for i = 1, n do
    local u, p = units$id[i % #units$id + 1], seps$id[(i // 2) % #seps$id + 1]
    local c = (i * $M) % $K - $Z
    local r = string.rep(u, c)
    local r2 = string.rep(u, c, p)
    acc = acc + #r + #r2 * 3
    if #r2 > 40 then ls = ls + 1 end
    $EXTRA
  end
  mix(acc); mix(ls)
end
run("brr$id", run$id, $N)
]==], { id = id, N = N(40, 500), M = pick{ 1, 3, 7, 11 }, K = rnd(20, 60), Z = rnd(0, 4),
        EXTRA = pick{ "", "if i % 41 == 0 then acc = acc + #string.rep('ab', 1 << 12) end", "if i % 19 == 0 then local ok = pcall(string.rep, 'x', 1 << 62) acc = acc + (ok and 1 or 0) end" } })
end
BRT[#BRT + 1] = function(id)
  return tpl([==[
local function run$id(n)
  local t = {}
  for j = 1, $P do t["key" .. j] = j end
  local acc, miss = 0, 0
  for i = 1, n do
    local k = "key" .. (i % $Q + 1)
    local v = t[k]
    if v then acc = acc + v else miss = miss + 1 end
    $EXTRA
  end
  mix(acc); mix(miss)
  local c = 0 for _ in pairs(t) do c = c + 1 end
  mix(c)
end
run("brk$id", run$id, $N)
]==], { id = id, N = N(40, 600), P = rnd(5, 25), Q = rnd(5, 30),
        EXTRA = pick{ "", "if i % 7 == 0 then t[k] = i end", "if i % 13 == 0 then t['k' .. i] = i end", "t['key' .. (i % 3 + 1)] = nil" } })
end

-- BUNDLE-N (JITDIFF_TSET=nr draws only these; not part of TT / T): callee/caller pairs for `RETF` (the `retf` knob): a callee loop with a trip count drawn from
-- {0, 1, 2, random}, called from a caller loop in a random shape (plain, tail, method, `__call`, `pcall`, `table.sort` comparator, `string.gsub` callback), random
-- result counts, an optional closure in the callee that captures a caller local (read-only or written), and an optional `RETURN` with `k`.
local NRT = {}
local function nr_trip() return pick{ "0", "1", "2", "(i % $M)", "(i % $M) + 1" } end
local function nr_callee(id, wanted)
  -- the callee body: trip count T, result count R (0..3), optional closure over the caller's local (`up`), optional k-RETURN
  return tpl([==[
local function cal$id(t, up)
  local s = $C
  for j = 1, t do s = s + j * $K end
  $CLO
  $RET
end
]==], { id = id, C = rnd(0, 3), K = rnd(1, 4),
        CLO = pick{ "", "", "if t > 1000 then local c = function() return s + up end s = c() end", "local c = function() return s end if t > 3 then s = s + c() end" },
        RET = pick{ "return s", "return s, t", "return s, t, s + t", "return", "return nil, s" } })
end
NRT[#NRT + 1] = function(id)
  return nr_callee(id) .. tpl([==[
local function run$id(n)
  local acc = 0
  for i = 1, n do
    local a, b, c = cal$id($T, i)
    acc = acc + (a or 0) + (b or 0) * 3 + (c or 0) * 5
    $POST
  end
  mix(acc)
end
run("nrp$id", run$id, $N)
]==], { id = id, N = N(60, 600), M = rnd(3, 9), T = nr_trip():gsub("%$M", tostring(rnd(3, 9))),
        POST = pick{ "", "if i % 7 == 0 then acc = acc + select('#', cal" .. id .. "(i % 3)) end", "local t = {cal" .. id .. "(i % 4)} acc = acc + #t" } })
end
NRT[#NRT + 1] = function(id)
  return nr_callee(id) .. tpl([==[
local function tl$id(t, u) return cal$id(t, u) end
local function tl2$id(t, u) return tl$id(t + 1, u) end
local function run$id(n)
  local acc = 0
  for i = 1, n do
    local a, b = tl$id($T, i)
    local c = tl2$id(i % 3, i)
    acc = acc + (a or 0) + (b or 0) + (c or 0)
  end
  mix(acc)
end
run("nrt$id", run$id, $N)
]==], { id = id, N = N(60, 600), T = nr_trip():gsub("%$M", tostring(rnd(3, 9))) })
end
NRT[#NRT + 1] = function(id)
  return nr_callee(id) .. tpl([==[
local obj$id = { n = $K }
function obj$id:m(t, u) local r, q = cal$id(t + self.n % 2, u) return (r or 0) + (q or 0) end
local callable$id = setmetatable({}, { __call = function(self, t, u) return cal$id(t, u) end })
local idx$id = setmetatable({}, { __index = function(tb, k) return (cal$id(k % 5, k)) end })
local function run$id(n)
  local acc = 0
  for i = 1, n do
    acc = acc + obj$id:m($T, i)
    local a = callable$id(i % 4, i)
    acc = acc + (a or 0) + idx$id[i % 7]
  end
  mix(acc)
end
run("nrm$id", run$id, $N)
]==], { id = id, N = N(60, 500), K = rnd(1, 5), T = nr_trip():gsub("%$M", tostring(rnd(3, 9))) })
end
NRT[#NRT + 1] = function(id)
  return nr_callee(id) .. tpl([==[
local function run$id(n)
  local acc = 0
  for i = 1, n do
    local ok, a, b = pcall(cal$id, $T, i)
    acc = acc + (ok and 1 or 0) + (a or 0) + (b or 0)
    local t = {}
    for j = 1, 6 do t[j] = (j * 7 + i) % 11 end
    table.sort(t, function(x, y) return (cal$id(x % 3, y) or 0) < (cal$id(y % 3, x) or 0) end)
    acc = acc + t[1] + t[6]
    local g = string.gsub("ab", "%w", function(ch) return (cal$id(i % 3, 1) or 0) % 2 == 0 and ch or nil end)
    acc = acc + #g
    $POST
  end
  mix(acc)
end
run("nrc$id", run$id, $N)
]==], { id = id, N = N(30, 200), T = nr_trip():gsub("%$M", tostring(rnd(3, 9))),
        POST = pick{ "", "local co = coroutine.wrap(function(x) local y = cal" .. id .. "(x % 4, x) coroutine.yield(y) return x end) acc = acc + (co(i) or 0)" } })
end
NRT[#NRT + 1] = function(id)
  return nr_callee(id) .. tpl([==[
local function run$id(n)
  local acc, cnt = 0, 0
  local function bump(t)
    local r = (cal$id(t, cnt)) or 0
    cnt = cnt + r % 3
    return r
  end
  for i = 1, n do
    local v = bump($T)
    acc = acc + v + cnt
    $POST
  end
  mix(acc); mix(cnt)
end
run("nru$id", run$id, $N)
]==], { id = id, N = N(60, 600), T = nr_trip():gsub("%$M", tostring(rnd(3, 9))),
        POST = pick{ "", "if i % 19 == 0 then local ok = pcall(error, 'x') acc = acc + (ok and 1 or 0) end", "if i % 23 == 0 then collectgarbage() end" } })
end
NRT[#NRT + 1] = function(id)
  return nr_callee(id) .. tpl([==[
local function va$id(...)
  local a, b = ...
  local r, q = cal$id((a or 0) % 4, b or 0)
  return (r or 0) + select('#', ...) + (q or 0)
end
local function deep$id(d, t) if d == 0 then return (cal$id(t, d)) or 0 end return 1 + deep$id(d - 1, t) end
local function run$id(n)
  local acc = 0
  for i = 1, n do
    acc = acc + va$id(i, i + 1, i + 2) + va$id()
    acc = acc + deep$id(i % 5, i % 4)
  end
  mix(acc)
end
run("nrv$id", run$id, $N)
]==], { id = id, N = N(60, 500) })
end

-- BUNDLE-N-B (N3a/N3b, JITDIFF_TSET=nr): callers that go on after the pop: two levels (`g` calls `f` and returns into the loop), the callee called twice (the
-- re-push into its loop head), a caller loop after the call (traced / holding a C call so that it stays untraced), recursion through the callee, an upvalue the
-- callee writes into the caller's frame, caller registers of other types than the callee's, a `debug.getlocal` / error after the pop.
NRT[#NRT + 1] = function(id)
  return nr_callee(id) .. tpl([==[
local function g$id(t, u) local a, b = cal$id(t, u) return (a or 0) + (b or 0) + $K end
local function h$id(t, u) return (g$id(t + 1, u)) * 2 end
local function twice$id(t, u)
  local a = cal$id(t, u)
  local b = cal$id(t + $K % 3, u)
  return (a or 0) * 3 + (b or 0)
end
local function run$id(n)
  local acc = 0
  for i = 1, n do
    acc = acc + g$id($T, i) + h$id(i % 3, i) + twice$id(i % 5, i)
    local str, fl = "s" .. (i % 4), i * 0.5
    local v = g$id(i % 4, i)
    acc = acc + #str + fl + v
  end
  mix(acc)
end
run("nr2$id", run$id, $N)
]==], { id = id, N = N(60, 600), K = rnd(1, 5), T = nr_trip():gsub("%$M", tostring(rnd(3, 9))) })
end
NRT[#NRT + 1] = function(id)
  return nr_callee(id) .. tpl([==[
local function tailpart$id(i, v)
  local s = 0
  for k = 1, 3 do s = s + v * k + i end
  return s
end
local function body$id(i, mode)
  local v = cal$id($T, i)
  if mode == 0 then
    local s = 0
    for k = 1, 4 do s = s + (v or 0) + k end
    return s
  elseif mode == 1 then
    local s = 0
    for k = 1, 3 do s = s + #tostring(k + (v or 0)) end
    return s
  end
  return tailpart$id(i, v or 0)
end
local function run$id(n)
  local acc = 0
  local up = 0
  local function bump() up = up + 1 end
  for i = 1, n do
    acc = acc + body$id(i, i % 3)
    bump()
    if i % 97 == 0 then
      local nm, val = debug.getlocal(1, 1)
      acc = acc + #tostring(nm) + (val or 0)
    end
    if i % 131 == 0 then local ok = pcall(error, "e" .. i) acc = acc + (ok and 1 or 0) end
  end
  mix(acc); mix(up)
end
run("nrl$id", run$id, $N)
]==], { id = id, N = N(60, 600), T = nr_trip():gsub("%$M", tostring(rnd(3, 9))) })
end
NRT[#NRT + 1] = function(id)
  return nr_callee(id) .. tpl([==[
local function fibr$id(n) if n < 2 then return n end return fibr$id(n - 1) + fibr$id(n - 2) end
local function mix2$id(t, u)
  local r = cal$id(t, u)
  local q = fibr$id(u % 6)
  local r2 = cal$id(t + 1, u)
  return (r or 0) + q + (r2 or 0)
end
local function run$id(n)
  local acc = 0
  for i = 1, n do
    acc = acc + mix2$id($T, i)
    if i % 41 == 0 then collectgarbage("step", 1) end
  end
  mix(acc)
end
run("nrr$id", run$id, $N)
]==], { id = id, N = N(60, 400), T = nr_trip():gsub("%$M", tostring(rnd(3, 9))) })
end

local nsnip = rnd(5, 9)
local used = {}
emit("-- snippets: " .. nsnip .. "\n")
for si = 1, nsnip do
  local id
  local text
  if tset == "oop" then
    -- P-A: only the `__index` chain templates (JITDIFF_TSET=oop)
    id = newid()
    text = TT[OOP0 + rnd(#TT - OOP0)](id)
  elseif tset == "sink" then
    -- SINK-F / SINK-R: only the allocation-sinking templates (JITDIFF_TSET=sink)
    id = newid()
    text = TT[SINKT[rnd(#SINKT)]](id)
  elseif tset == "dm" then
    -- DYNMASK: only the dynamic-key templates (JITDIFF_TSET=dm)
    id = newid()
    text = TT[DMT[rnd(#DMT)]](id)
  elseif tset == "t1i" then
    -- BUNDLE-T1I: only the inline built-in call template (JITDIFF_TSET=t1i)
    id = newid()
    text = TT[T1IT[rnd(#T1IT)]](id)
  elseif tset == "cmp" then
    -- BUNDLE-B: only the comparison-metamethod templates (JITDIFF_TSET=cmp)
    id = newid()
    text = CMPT[rnd(#CMPT)](id)
  elseif tset == "bc" then
    -- BUNDLE-C: only the recursion-shape templates of `sbump` / `fnpromo` (JITDIFF_TSET=bc)
    id = newid()
    text = BCT[rnd(#BCT)](id)
  elseif tset == "gm" then
    -- BUNDLE-GM: only the `gmatch` iterator templates (JITDIFF_TSET=gm)
    id = newid()
    text = GMT[rnd(#GMT)](id)
  elseif tset == "ba" then
    -- BUNDLE-A: only the `tonumber` / `next` / `find` templates (JITDIFF_TSET=ba)
    id = newid()
    text = BAT[rnd(#BAT)](id)
  elseif tset == "cc" then
    -- BUNDLE-C: only the `string.format` / light C call templates (JITDIFF_TSET=cc)
    id = newid()
    text = CCT[rnd(#CCT)](id)
  elseif tset == "bd" then
    -- BUNDLE-D: only the arithmetic-metamethod / C-stack-depth templates (JITDIFF_TSET=bd)
    id = newid()
    text = BDT[rnd(#BDT)](id)
  elseif tset == "bf" then
    -- BUNDLE-F: only the stitch / native-recursion templates (JITDIFF_TSET=bf)
    id = newid()
    text = BFT[rnd(#BFT)](id)
  elseif tset == "bg" then
    -- BUNDLE-G: only the wide polymorphic call-site templates (JITDIFF_TSET=bg)
    id = newid()
    text = BGT[rnd(#BGT)](id)
  elseif tset == "bh" then
    -- BUNDLE-H: only the library-call / traversal templates (JITDIFF_TSET=bh)
    id = newid()
    text = BHT[rnd(#BHT)](id)
  elseif tset == "bj" then
    -- BUNDLE-J: only the C-entered function templates (JITDIFF_TSET=bj)
    id = newid()
    text = BJT[rnd(#BJT)](id)
  elseif tset == "bk" then
    -- BUNDLE-K: only the lazy-CallInfo recursion / observer templates (JITDIFF_TSET=bk)
    id = newid()
    text = BKT[rnd(#BKT)](id)
  elseif tset == "bl" then
    -- BUNDLE-L: only the array-loop templates (JITDIFF_TSET=bl)
    id = newid()
    text = BLT[rnd(#BLT)](id)
  elseif tset == "bt" then
    -- BUNDLE-T: only the coroutine switch templates (JITDIFF_TSET=bt)
    id = newid()
    text = BTT[rnd(#BTT)](id)
  elseif tset == "br" then
    -- BUNDLE-R: only the library-call templates (JITDIFF_TSET=br)
    id = newid()
    text = BRT[rnd(#BRT)](id)
  elseif tset == "nr" then
    -- BUNDLE-N: only the callee/caller templates of `RETF` (JITDIFF_TSET=nr)
    id = newid()
    text = NRT[rnd(#NRT)](id)
  elseif tset == "bs" then
    -- BUNDLE-S: only the metatabled-array / Lua `__index` `__newindex` frame templates (JITDIFF_TSET=bs)
    id = newid()
    text = BST[rnd(#BST)](id)
  elseif tset == "bv" then
    -- BUNDLE-V: only the arithmetic metamethod frame / tail call in metamethod frame / upvalue barrier templates (JITDIFF_TSET=bv)
    id = newid()
    text = BVT[rnd(#BVT)](id)
  elseif tset == "bq" then
    -- BUNDLE-Q: only the object-equality / float-modulo templates of `t1q` (JITDIFF_TSET=bq)
    id = newid()
    text = BQT[rnd(#BQT)](id)
  elseif tset == "bp" then
    -- BUNDLE-P: only the float-floor / class-dispatch / metamethod tail-call templates (JITDIFF_TSET=bp)
    id = newid()
    text = BPT[rnd(#BPT)](id)
  elseif tset == "pd" then
    -- P3: only the polymorphic call-site templates (JITDIFF_TSET=pd)
    id = newid()
    text = PDT[rnd(#PDT)](id)
  elseif tset == "q" then
    -- BUNDLE-Q: only the generic-for-over-closure template (JITDIFF_TSET=q)
    id = newid()
    text = TT[#TT](id)
  elseif tset == "tr" and chance(75) then
    id = newid()
    -- BC: the BB-2 template (tier-1 SETTABLE stores) is drawn in about one snippet of eight, besides its share of the uniform pick
    text = (chance(12) and TT[BB2] or TT[rnd(#TT)])(id)
  else
    local k = rnd(#T)
    id = newid()
    text = T[k](id)
  end
  if not keep or keep[si] then emit(text) end
end

-- tail
if hookmode then emit("debug.sethook()\nprint('hooks', HC, HL, HLH)\n") end
emit("print('final', H)\n")
local tailk = rnd(10)
if tailk == 1 then emit("error({code = 1})\n")
elseif tailk == 2 then emit("error('final error')\n")
elseif tailk == 3 then emit("local t = nil; t.x = 1\n")
elseif tailk == 4 then emit("os.exit(3)\n")
elseif tailk == 5 then emit("os.exit(true)\n")
elseif tailk == 6 then emit("error('lvl0', 0)\n")
elseif tailk == 7 then emit("error(setmetatable({}, {__tostring = function() return 'tostr' end}))\n")
end

io.write(table.concat(out))
