-- BUNDLE-B: `<`, `<=`, `==` on tables / userdata with Lua-function `__lt` / `__le` / `__eq` in hot (tier-1 compiled) code:
-- swapped operands, nil / false / number results, errors, yields and hooks inside the metamethod, mixed types, a metamethod on
-- one operand only, comparator errors inside table.sort, deep recursion through the metamethod.
local out = {}
local function P(...)
  local t = {}
  for i = 1, select("#", ...) do t[i] = tostring((select(i, ...))) end
  out[#out + 1] = table.concat(t, " ")
end
local function msg(e) return (tostring(e):gsub("^.-:%d+: ", "")) end

local N = 400  -- enough for entry / loop thresholds of every JIT mode

local calls = 0
local mt = {}
mt.__lt = function(a, b) calls = calls + 1; return a.v < b.v end
mt.__le = function(a, b) calls = calls + 1; return a.v <= b.v end
mt.__eq = function(a, b) calls = calls + 1; return a.v == b.v end
local function new(v) return setmetatable({ v = v }, mt) end

local function lt(a, b) return a < b end
local function le(a, b) return a <= b end
local function gt(a, b) return a > b end
local function ge(a, b) return a >= b end
local function eq(a, b) return a == b end
local function ne(a, b) return a ~= b end
local function iflt(a, b) if a < b then return "y" else return "n" end end
local function ifnle(a, b) if not (a <= b) then return "y" else return "n" end end

-- (1) basic, swapped operands, all forms; the results are accumulated so that they are compared as one line
local x, y = new(1), new(2)
for round = 1, 3 do
  local acc = {}
  for i = 1, N do
    local a, b = x, y
    if i % 3 == 0 then a, b = y, x end
    if i % 7 == 0 then a, b = x, x end
    acc[#acc + 1] = (lt(a, b) and "1" or "0") .. (le(a, b) and "1" or "0") .. (gt(a, b) and "1" or "0") .. (ge(a, b) and "1" or "0")
      .. (eq(a, b) and "1" or "0") .. (ne(a, b) and "1" or "0") .. iflt(a, b) .. ifnle(a, b)
  end
  local counts = {}
  for _, s in ipairs(acc) do counts[s] = (counts[s] or 0) + 1 end
  local keys = {}
  for k in pairs(counts) do keys[#keys + 1] = k end
  table.sort(keys)
  for _, k in ipairs(keys) do P("basic", round, k, counts[k]) end
  P("calls", calls)
end

-- (2) results of every type: nil, false, numbers (truthy), strings, tables
local rv = {}
local rmt = { __lt = function() return rv[1] end, __le = function() return rv[2] end, __eq = function() return rv[3] end }
local r1, r2 = setmetatable({}, rmt), setmetatable({}, rmt)
for _, v in ipairs({ "nil", "false", "zero", "str", "tbl", "true" }) do
  local val
  if v == "nil" then val = nil elseif v == "false" then val = false elseif v == "zero" then val = 0 elseif v == "str" then val = "" elseif v == "tbl" then val = {} else val = true end
  rv[1], rv[2], rv[3] = val, val, val
  local n = 0
  for i = 1, N do
    if r1 < r2 then n = n + 1 end
    if r1 <= r2 then n = n + 10 end
    if r1 == r2 then n = n + 100 end
    if not (r1 < r2) then n = n + 1000 end
    if r1 ~= r2 then n = n + 10000 end
  end
  P("results", v, n)
end

-- (3) a metamethod on one operand only, in either position; __eq of different metatables; __le absent with __lt present
local mA = { __lt = function(a, b) return true end, __le = function(a, b) return false end }
local mB = { __eq = function(a, b) return true end }
local A, B = setmetatable({}, mA), setmetatable({}, mB)
local C = {}
local function cmp6(a, b)
  local ok, r = pcall(function() return (a < b) and 1 or 0 end)
  local s = ok and r or ("E:" .. msg(r))
  ok, r = pcall(function() return (a <= b) and 1 or 0 end)
  s = s .. "|" .. (ok and r or ("E:" .. msg(r)))
  ok, r = pcall(function() return (a == b) and 1 or 0 end)
  return s .. "|" .. (ok and r or ("E:" .. msg(r)))
end
for i = 1, 60 do
  if i == 60 then
    P("one", cmp6(A, C)); P("one", cmp6(C, A)); P("one", cmp6(A, 1)); P("one", cmp6(1, A)); P("one", cmp6(A, "x")); P("one", cmp6("x", A))
    P("one", cmp6(B, C)); P("one", cmp6(C, B)); P("one", cmp6(A, B)); P("one", cmp6(B, A)); P("one", cmp6(C, C)); P("one", cmp6({}, {}))
    P("one", cmp6(1, {})); P("one", cmp6({}, "x")); P("one", cmp6(nil, {})); P("one", cmp6(true, false))
  else
    cmp6(A, C); cmp6(C, A); cmp6(B, C); cmp6(A, B)
  end
end

-- (4) errors thrown inside the metamethod (message position, traceback shape), in hot code
local emt = { __lt = function(a, b) error("lt boom " .. tostring(a.v)) end, __le = function(a, b) error({ code = a.v }) end,
              __eq = function(a, b) local z = nil; return z.x end }
local e1, e2 = setmetatable({ v = 1 }, emt), setmetatable({ v = 2 }, emt)
for i = 1, N do
  local ok, e = pcall(lt, e1, e2)
  local ok2, e2_ = pcall(le, e1, e2)
  local ok3, e3 = pcall(eq, e1, e2)
  if i == N then P("err", ok, msg(e), ok2, type(e2_) == "table" and e2_.code, ok3, msg(e3)) end
end
local ok, tb = xpcall(lt, debug.traceback, e1, e2)
P("tb", (tb:gsub("%[C%]: in ", "[C]: in "):gsub("[^\n]*tier1_compare[^\n]*", "<src>"):gsub("\n.*", "")))
-- the error line seen by debug.getinfo from inside the metamethod
local seen = {}
local imt = { __lt = function(a, b)
  local i1 = debug.getinfo(1, "l"); local i2 = debug.getinfo(2, "l")
  seen[#seen + 1] = (i2.currentline - i1.currentline); return true end }
local q1, q2 = setmetatable({}, imt), setmetatable({}, imt)
local function viaq(a, b)
  return a < b
end
for i = 1, N do viaq(q1, q2) end
P("lines", #seen, seen[1], seen[N])

-- (5) yield inside the metamethod, called from hot code (and resumed from different places)
local ymt = {}
ymt.__lt = function(a, b) local r = coroutine.yield("lt") ; return r end
ymt.__le = function(a, b) coroutine.yield("le"); return a.v <= b.v end
ymt.__eq = function(a, b) local r = coroutine.yield("eq"); return r end
local function yv(v) return setmetatable({ v = v }, ymt) end
local function body(n)
  local y1, y2 = yv(1), yv(2)
  local acc = {}
  for i = 1, n do
    local a = lt(y1, y2)
    local b = le(y1, y2)
    local c = eq(y1, y2)
    local d = iflt(y2, y1)
    acc[#acc + 1] = (a and "T" or "F") .. (b and "T" or "F") .. (c and "T" or "F") .. d
  end
  return table.concat(acc, ",")
end
for round = 1, 4 do
  local co = coroutine.wrap(function() return body(N // 4) end)
  local tag, steps = co(), 0
  while true do
    steps = steps + 1
    local arg
    if tag == "lt" then arg = (steps % 3 == 0) elseif tag == "eq" then arg = (steps % 2 == 0) else arg = nil end
    local r = co(arg)
    if type(r) == "string" and #r > 8 then tag = nil; P("yield", round, steps, #r, r:sub(1, 40)); break end
    tag = r
  end
end

-- (6) hooks: a line / call / return hook installed while the loop runs; the metamethod frames are reported as in the interpreter
local hmt = { __lt = function(a, b) return a.v < b.v end }
local h1, h2 = setmetatable({ v = 1 }, hmt), setmetatable({ v = 2 }, hmt)
local ev = {}
for i = 1, N do
  if i == N - 3 then debug.sethook(function(e) ev[#ev + 1] = e end, "cr") end
  local r = lt(h1, h2)
  if i == N - 1 then debug.sethook() end
end
P("hook", #ev, table.concat(ev, ",", 1, math.min(#ev, 12)))

-- (7) table.sort comparators that use the metamethods, errors inside the comparator, invalid order functions
local list = {}
for i = 1, 200 do list[i] = new((i * 37) % 101) end
table.sort(list)
local okk = true
for i = 2, #list do if list[i].v < list[i - 1].v then okk = false end end
P("sort", okk, list[1].v, list[#list].v)
table.sort(list, function(a, b) if a < b then return false elseif b < a then return true end return false end)
P("sortdesc", list[1].v, list[#list].v)
local cnt = 0
local ok, e = pcall(table.sort, list, function(a, b) cnt = cnt + 1; if cnt > 150 then error("cmp boom") end return a < b end)
P("sorterr", ok, msg(e))
setmetatable(list[3], { __lt = function() error("inner lt") end })
local ok, e = pcall(table.sort, list)
P("sorterr2", ok, msg(e))

-- (8) deep recursion through the metamethod (C-stack accounting), then a normal run
local dmt = {}
local depth = 0
dmt.__lt = function(a, b) depth = depth + 1; return a < b end
local d1, d2 = setmetatable({}, dmt), setmetatable({}, dmt)
local ok, e = pcall(lt, d1, d2)
P("deep", ok, msg(e):sub(1, 40), depth > 100)

-- (9) metamethod changes the state the compare's caller reads (upvalue in a register of the caller)
local function chg()
  local n = 0
  local m = setmetatable({}, { __lt = function() n = n + 1; return n % 2 == 0 end })
  local o = setmetatable({}, getmetatable(m))
  local acc = {}
  for i = 1, 20 do
    local before = n
    local r = m < o
    acc[#acc + 1] = (r and 1 or 0) * 10 + (n - before)
  end
  return table.concat(acc, ",")
end
for i = 1, 60 do chg() end
P("upv", chg())

-- (10) userdata with __eq / __lt (io files get a metatable; make a custom one through a table-wrapped proxy)
local f1, f2 = io.stdout, io.stderr
local c = 0
for i = 1, N do if f1 == f2 then c = c + 1 end; if f1 == f1 then c = c + 1000 end end
P("udata", c)

-- (11) metamethods replaced / removed while hot
local rm = { __lt = function() return true end }
local u1, u2 = setmetatable({}, rm), setmetatable({}, rm)
local n = 0
for i = 1, N do
  if i == N // 2 then rm.__lt = function() return false end end
  if i == N - 10 then rm.__lt = nil end
  local ok, r = pcall(lt, u1, u2)
  if ok and r then n = n + 1 elseif not ok then n = n + 1000 end
end
P("replace", n)

-- (12) strings / numbers mixed in the same site stay correct
local mix = { 1, 2.5, "a", "b", new(3), new(4), 7, "zz" }
local s = {}
for i = 1, N do
  local a, b = mix[i % 8 + 1], mix[(i * 3) % 8 + 1]
  local ok, r = pcall(lt, a, b)
  local ok2, r2 = pcall(eq, a, b)
  if i > N - 16 then s[#s + 1] = tostring(ok and r or "E") .. tostring(ok2 and r2 or "E") end
end
P("mix", table.concat(s, " "))

print(table.concat(out, "\n"))
