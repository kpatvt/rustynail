-- Section 77.3 (`uvk`): read-only upvalues (captured locals never assigned after their initialisation) read as constants by traces that hold
-- the closure by identity. Every way the value seen through a closure can change is exercised mid-run: another closure of the prototype,
-- debug.setupvalue, debug.setlocal, debug.upvaluejoin, a collected closure whose address is reused, variables assigned only in a nested
-- function (not read-only), loop variables captured per iteration. Every loop sits inside a function (the tr_core harness convention).

local sbyte = string.byte
local B0, B9, BA, BZ = sbyte("0"), sbyte("9"), sbyte("a"), sbyte("z")
local KW = { ["if"] = true, ["end"] = true }
local NAME, FL, YES, NOTHING = "name", 0.5, true, nil

local function is_digit(b) return b >= B0 and b <= B9 end
local function is_alpha(b) return b >= BA and b <= BZ end

-- (1) a chain of read-only closures (is_alpha / is_digit), tables / strings / floats / booleans / nil
local function scan(s)
  local d, a, k, x = 0, 0, 0, 0.0
  for i = 1, #s do
    local b = sbyte(s, i)
    if is_digit(b) then d = d + 1 elseif is_alpha(b) then a = a + 1 end
    if KW[NAME] == nil and YES and NOTHING == nil then x = x + FL end
    k = k + 1
  end
  return d, a, k, x
end
local function case1()
  local text = string.rep("if x1 then y22 end ", 40)
  for r = 1, 30 do
    local d, a, k, x = scan(text)
    if r % 10 == 0 then print("scan", r, d, a, k, x) end
  end
end
case1()

-- (2) a factory: closures of one prototype capturing different read-only values, called from one hot site
local function make(k, name)
  return function(v) return v * k, name end
end
local function case2()
  local fs = {}
  for i = 1, 6 do fs[i] = make(i, "f" .. i) end
  local acc = 0
  for r = 1, 3000 do
    local f = fs[(r % 7 < 5) and 1 or (r % 6 + 1)]
    local v, n = f(r)
    acc = acc + v + #n
  end
  print("factory", acc)
end
case2()

-- (3) debug.setupvalue of a read-only upvalue between hot runs and from inside the hot loop
local LIMIT = 10
local function under(v) return v < LIMIT end
local function count_under(n)
  local c = 0
  for i = 1, n do if under(i % 50) then c = c + 1 end end
  return c
end
local function case3()
  print("setupvalue0", count_under(2000))
  debug.setupvalue(under, 1, 30)
  print("setupvalue1", count_under(2000))
  local c3 = 0
  for i = 1, 3000 do
    if i % 700 == 0 then debug.setupvalue(under, 1, (i // 700) * 5) end
    if under(i % 50) then c3 = c3 + 1 end
  end
  print("setupvalue2", c3)
  for k = 1, 5 do
    debug.setupvalue(under, 1, k * 7)
    print("setupvalue3", k, count_under(500))
  end
end

-- (4) debug.setlocal of the declaring frame's register while the upvalue is open
local function run4(step, n)
  local s = 0
  for i = 1, n do s = step(s) end
  return s
end
local function case4()
  local STEP = 3
  local function step(v) return v + STEP end
  print("setlocal0", run4(step, 1000))
  local i = 1
  while true do
    local name = debug.getlocal(1, i)
    if not name then break end
    if name == "STEP" then debug.setlocal(1, i, 7) end
    i = i + 1
  end
  print("setlocal1", run4(step, 1000))
end

-- (5) debug.upvaluejoin: a read-only upvalue joined to a variable that is assigned
local function sum5(get, n)
  local s = 0
  for i = 1, n do s = s + get() end
  return s
end
local function case5()
  local RO = 5
  local mut = 100
  local function get() return RO end
  local function bump() mut = mut + 1 end
  print("join0", sum5(get, 1000))
  debug.upvaluejoin(get, 1, bump, 1)
  print("join1", sum5(get, 1000))
  for i = 1, 500 do bump() end
  print("join2", sum5(get, 1000))
  local s = 0
  for i = 1, 2000 do
    if i % 3 == 0 then bump() end
    s = s + get()
  end
  print("join3", s)
end

-- (6) closures collected and their addresses reused by closures of the same prototype with other values
local function mk6(k) return function(v) return v + k end end
local function loop6(holder)
  local s = 0
  for i = 1, 200 do s = holder.f(s) end
  return s
end
local function case6()
  local holder = {}
  local total = 0
  for round = 1, 60 do
    holder.f = mk6(round)
    total = total + loop6(holder)
    holder.f = nil
    collectgarbage()
  end
  print("reuse", total)
end
case6()

-- (7) assigned only in a nested function: not read-only
local function case7()
  local x = 1
  local function getx() return x end
  local function incx() x = x + 1 end
  local s = 0
  for i = 1, 3000 do
    s = s + getx()
    if i % 100 == 0 then incx() end
  end
  print("nested-assign", s, x)
end
case7()

-- (8) loop variables captured per iteration, and a local assigned in its own function after the closure exists
local function case8()
  local cl = {}
  for i = 1, 5 do cl[i] = function() return i * 10 end end
  local s = 0
  for r = 1, 2000 do s = s + cl[r % 5 + 1]() end
  print("loopvar", s)
  local y = 2
  local function gety() return y end
  local t = 0
  for r = 1, 2000 do
    t = t + gety()
    if r == 1000 then y = 9 end
  end
  print("late-assign", t)
end
case8()

-- (9) a recursive local function read through its own upvalue, with read-only constants
local function case9()
  local BASE = 2
  local function fib(n) if n < BASE then return n end return fib(n - 1) + fib(n - 2) end
  local s = 0
  for i = 1, 20 do s = s + fib(12) end
  print("fib", s)
end
case9()

-- (10) a closure made by the main chunk, recorded while it is the only one of its prototype, then a second one at the same call site
local function run10(holder)
  local s = 0
  for i = 1, 300 do s = holder.f(s) end
  return s
end
local holder10 = {}
for round = 1, 3 do
  local K = round * 10
  holder10.f = function(v) return v + K end
  print("mainmade", round, run10(holder10))
end

-- the cases that write read-only variables outside the language run last (each one flushes; the join stops folding for good)
case3()
case4()
case5()
