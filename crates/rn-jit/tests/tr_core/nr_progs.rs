//! BUNDLE-N (N2c): programs whose callee `RETURN`s pop into a caller that a side trace continues in (`retf` knob, docs/JIT_PARKED.md C38).

use super::progs::Prog;

pub const NR_PROGS: &[Prog] = &[
    Prog {
        name: "n_retf_basic",
        small: true,
        src: r####"
-- a callee loop with a data-dependent trip count, called from a caller loop; nested calls; calls in loops
local function f(n)
  local s = 0
  for i = 1, n do s = s + i * 2 end
  return s, n
end
local function g(n) local a, b = f(n) return a + b end
local function h(n) return g(n) + g(n + 1) end
local acc = 0
for r = 1, 3000 do
  local a, b = f(r % 7)
  acc = acc + a + b + g(r % 5) + h(r % 3)
  if r % 750 == 0 then print(r, acc) end
end
local t = {}
for r = 1, 600 do
  for j = 0, 2 do t[#t + 1] = g((r + j) % 6) end
end
local c = 0
for i = 1, #t do c = c + t[i] end
print(acc, #t, c)
"####,
    },
    Prog {
        name: "n_retf_tail",
        small: true,
        src: r####"
-- the callee is reached by TAILCALL (CIST_TAIL is masked out of the callstatus guard)
local function g(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function f(n) return g(n) end
local function f2(n) return f(n + 1) end
local acc = 0
for r = 1, 3000 do
  local a = f(r % 9)
  local b = f2(r % 4)
  acc = acc + a + b
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_multret",
        small: true,
        src: r####"
-- results consumed by multi-result sequences: {f()}, f(g()), return g(), select('#', f())
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  if n % 2 == 0 then return s, n, s + n end
  return s
end
local function g(...) return select('#', ...), ... end
local function k(n) return f(n) end
local acc = 0
for r = 1, 2500 do
  local t = {f(r % 6)}
  local n = select('#', f(r % 5))
  local u = {g(f(r % 4))}
  local a, b, c = k(r % 6)
  acc = acc + #t + n + #u + (a or 0) + (b or 0) + (c or 0)
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_ccaller",
        small: true,
        src: r####"
-- the same callee called from Lua, table.sort, pcall, string.gsub and a coroutine: the guards fail for the C callers and the exit happens before the RETURN
local function f(a, b)
  local s = 0
  for i = 1, 3 do s = s + i end
  return a < b
end
local function lt(a, b) return f(a, b) end
local acc = 0
for r = 1, 400 do
  local t = {}
  for i = 1, 12 do t[i] = (i * 7 + r) % 13 end
  table.sort(t, f)
  acc = acc + t[1] + t[12]
  local ok, v = pcall(f, r, 200)
  if ok and v then acc = acc + 1 end
  local s = string.gsub("abc", "%w", function(c) if f(1, 2) then return c:upper() end end)
  acc = acc + #s
  local co = coroutine.wrap(function(x) local y = f(x, 100) coroutine.yield(y) return x end)
  if co(r) then acc = acc + 1 end
  if lt(r, 100) then acc = acc + 2 end
  if f(r, 100) then acc = acc + 3 end
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_two_sites",
        small: true,
        src: r####"
-- two call sites alternate: the site guard of a RETF trace fails for the other one
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function a(n) local x = f(n) return x + 1 end
local function b(n) local x = f(n) return x * 2 end
local acc = 0
for r = 1, 3000 do
  if r % 3 == 0 then acc = acc + a(r % 8) else acc = acc + b(r % 6) end
  local y = f(r % 5)
  acc = acc + y
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_mm",
        small: true,
        src: r####"
-- callees reached through __call, an __index function and an arithmetic metamethod (CIST_MMRET frames refuse the guards)
local C = setmetatable({}, {__call = function(self, n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end})
local I = setmetatable({}, {__index = function(t, k)
  local s = 0
  for i = 1, k do s = s + i end
  return s
end})
local A = setmetatable({}, {__add = function(x, y)
  local s = 0
  for i = 1, y do s = s + i end
  return s
end})
local acc = 0
for r = 1, 2500 do
  acc = acc + C(r % 6) + I[r % 7] + (A + (r % 5))
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_getlocal",
        small: true,
        src: r####"
-- an error right after the pop: the handler walks the caller's frame (debug.getlocal / getinfo / traceback see the interpreter's frames)
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function caller(n, bad)
  local x = f(n)
  local y = x * 2
  if bad then error("boom " .. y) end
  return y
end
local log = {}
local function handler(m)
  local names = {}
  for i = 1, 6 do
    local nm, v = debug.getlocal(2, i)
    if nm then names[#names + 1] = nm .. "=" .. tostring(v) end
  end
  local info = debug.getinfo(2, "l")
  return m .. " | " .. table.concat(names, ",") .. " | line " .. tostring(info.currentline) .. " | " .. (debug.traceback("tb", 1):gsub("\n.*", ""))
end
local acc = 0
for r = 1, 1500 do
  local ok, m = xpcall(caller, handler, r % 6, r % 100 == 0)
  if not ok then log[#log + 1] = m else acc = acc + m end
end
print(acc, #log)
for i = 1, math.min(#log, 3) do print(log[i]) end
"####,
    },
    Prog {
        name: "n_retf_hook",
        small: true,
        src: r####"
-- a hook installed in the middle of the run: count and line events see the exact instruction counts after the pop
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local acc, ev = 0, 0
for r = 1, 3000 do
  acc = acc + f(r % 7)
  if r == 1500 then debug.sethook(function() ev = ev + 1 end, "", 7) end
  if r == 2000 then debug.sethook() end
end
print(acc, ev > 0)
"####,
    },
    Prog {
        name: "n_retf_vararg_caller",
        small: true,
        src: r####"
-- the caller is a vararg function (its function slot is shifted), and the callee is not
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function v(...)
  local a, b = ...
  local x = f((a or 0) % 7)
  return x + select('#', ...) + (b or 0)
end
local acc = 0
for r = 1, 3000 do
  acc = acc + v(r, r + 1, r + 2) + v(r) + v()
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_closeret",
        small: true,
        src: r####"
-- callees with open upvalues (closures returned), a to-be-closed variable, and a RETURN that closes: the k-guards
local function mk(n)
  local c = 0
  local function inc() c = c + 1 return c end
  for i = 1, n do inc() end
  return inc
end
local function tb(n)
  local closed = false
  local x <close> = setmetatable({}, {__close = function() closed = true end})
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function plain(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s, function() return s end
end
local function lazyk(n)
  local s = 0
  for i = 1, n do s = s + i end
  if n > 1000 then local c = function() return s end s = c() end
  return s
end
local acc = 0
for r = 1, 2500 do
  acc = acc + lazyk(r % 7)
  local g = mk(r % 5)
  acc = acc + g() + tb(r % 6)
  local a, h = plain(r % 7)
  acc = acc + a + h()
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_coro",
        small: true,
        src: r####"
-- coroutine base frames (the first Lua frame of a coroutine has a C frame below it) and calls inside coroutines
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local acc = 0
local co = coroutine.wrap(function(a)
  for r = 1, 2000 do
    local x = f(r % 6)
    acc = acc + x
    if r % 100 == 0 then a = coroutine.yield(r) end
  end
  return -1
end)
local last
repeat last = co(0) until last == -1
for r = 1, 2000 do acc = acc + f(r % 6) end
local c2 = coroutine.create(f)
local ok, v = coroutine.resume(c2, 5)
print(acc, ok, v)
"####,
    },
    Prog {
        name: "n_retf_deep",
        small: true,
        src: r####"
-- deep recursion with callee returns at every level (stack reallocation, CallInfo reuse)
local function f(n)
  local s = 0
  for i = 1, n % 4 do s = s + i end
  return s
end
local function rec(n)
  if n == 0 then return 0 end
  local x = f(n)
  local y = rec(n - 1)
  return x + y + 1
end
local acc = 0
for r = 1, 60 do acc = acc + rec(80 + r % 5) end
local function deep(n) if n == 0 then return f(3) end return 1 + deep(n - 1) end
acc = acc + deep(3000)
print(acc)
"####,
    },
    Prog {
        name: "n_retf_gc",
        small: true,
        src: r####"
-- allocation in the callee and the caller around the pop; collections at exits
local function f(n)
  local t = {}
  for i = 1, n do t[i] = {i} end
  return t, #t
end
local acc = 0
for r = 1, 1500 do
  local t, n = f(r % 6)
  local u = {}
  for i = 1, n do u[i] = t[i][1] * 2 end
  acc = acc + #u + (u[1] or 0)
  if r % 300 == 0 then collectgarbage() end
end
print(acc, collectgarbage("count") > 0)
"####,
    },
    Prog {
        name: "n_retf_results",
        small: true,
        src: r####"
-- different numbers of wanted results for the same callee (0, 1, 2, 3, multi) and fewer/more returned values, one loop per shape
local function f(n)
  if n % 3 == 0 then return end
  if n % 3 == 1 then return n end
  return n, n * 2
end
local acc = 0
for r = 1, 2000 do f(r) end
for r = 1, 2000 do local a = f(r) acc = acc + (a or 0) end
for r = 1, 2000 do local b, c = f(r) acc = acc + (b or 0) + (c or 0) end
for r = 1, 2000 do local d, e, g = f(r) acc = acc + (d or 0) + (e or 0) + (g or 0) end
for r = 1, 2000 do local t = {f(r)} acc = acc + #t end
for r = 1, 2000 do acc = acc + select('#', f(r)) end
for r = 1, 2000 do acc = acc + select('#', f(r), f(r + 1)) end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_upvalue_alias",
        small: true,
        src: r####"
-- the callee writes a local of the caller through an upvalue, then the caller reads it
local function run(n)
  local x = 0
  local function bump(k)
    local s = 0
    for i = 1, k do s = s + i end
    x = x + s
    return s
  end
  local acc = 0
  for r = 1, n do
    local v = bump(r % 5)
    acc = acc + v + x
  end
  return acc, x
end
print(run(3000))
"####,
    },
    Prog {
        name: "n_retf_continue",
        small: true,
        src: r####"
-- N3a: the recording goes on in the caller after the pop and links to the caller's traced loop head (plain link)
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local acc, t = 0, {}
for r = 1, 4000 do
  local v = f(r % 4)
  local w = v * 2 + r
  if w % 7 == 0 then t[#t + 1] = w end
  acc = acc + w
end
print(acc, #t)
-- the same with the caller loop inside another function called repeatedly
local function g(m)
  local a = 0
  for r = 1, m do a = a + f(r % 3) + 1 end
  return a
end
local z = 0
for k = 1, 600 do z = z + g(k % 9) end
print(z)
"####,
    },
    Prog {
        name: "n_retf_untraced",
        small: true,
        src: r####"
-- N3a: the caller's loop contains something the trace compiler cannot record (an untraced loop head after the pop)
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local acc = 0
for r = 1, 3000 do
  local v = f(r % 5)
  for j = 1, 2 do
    acc = acc + v + #tostring(os.time() > 0) + j
  end
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_relink",
        small: true,
        src: r####"
-- N3a/N3b: lexer shape: the callee scans in a loop and returns a token; the caller loop calls it again (the callee's loop head is re-entered
-- by a pushed frame after the pop)
local src = string.rep("ab cde  f ghij kl m  nopqr ", 40)
local Lex = {}
Lex.__index = Lex
function Lex.new(s) return setmetatable({s = s, pos = 1}, Lex) end
function Lex:next()
  local s, p = self.s, self.pos
  while true do
    local c = s:byte(p)
    if c == nil then self.pos = p return nil end
    if c ~= 32 then break end
    p = p + 1
  end
  local q = p
  while true do
    local c = s:byte(q)
    if c == nil or c == 32 then break end
    q = q + 1
  end
  self.pos = q
  return q - p
end
local total, cnt = 0, 0
for rep = 1, 40 do
  local lx = Lex.new(src)
  while true do
    local n = lx:next()
    if not n then break end
    total = total + n
    cnt = cnt + 1
  end
end
print(total, cnt)
"####,
    },
    Prog {
        name: "n_retf_recursion",
        small: true,
        src: r####"
-- the caller reached after the pop calls the callee again, which recurses
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local function f(n)
  local s = 0
  for i = 1, n do s = s + fib(i % 6) end
  return s
end
local acc = 0
for r = 1, 3000 do acc = acc + f(r % 5) + f((r + 1) % 4) end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_two_levels",
        small: true,
        src: r####"
-- N3a: two RETURNs in a row (retfmax = 2): f returns into g, g returns into the loop
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function g(n) local a = f(n) return a + 1 end
local function h(n) return (g(n)) * 2 end
local acc = 0
for r = 1, 4000 do acc = acc + g(r % 5) + h(r % 3) end
print(acc)
local function k(n) local x = g(n) local y = g(n + 1) return x + y end
local z = 0
for r = 1, 3000 do z = z + k(r % 6) end
print(z)
"####,
    },
    Prog {
        name: "n_retf_after_exits",
        small: true,
        src: r####"
-- exits taken after the pop: errors, tracebacks, debug.getinfo / getlocal in the caller, collections
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function caller(r)
  local loc1, loc2 = r, r * 2
  local v = f(r % 4)
  if r % 997 == 0 then
    local info = debug.getinfo(1, "nSl")
    local nm, val = debug.getlocal(1, 1)
    return v + info.currentline * 0 + (val or 0) * 0 + #debug.traceback("x") * 0 + loc2 * 0
  end
  if r % 1499 == 0 then error("boom " .. r) end
  if r % 211 == 0 then collectgarbage("step", 1) end
  return v + loc1 * 0
end
local acc, errs = 0, 0
for r = 1, 6000 do
  local ok, v = pcall(caller, r)
  if ok then acc = acc + v else errs = errs + 1 end
end
print(acc, errs)
local acc2 = 0
for r = 1, 6000 do
  local v = f(r % 3)
  if r % 1000 == 0 then local _, x = debug.getlocal(1, 1) acc2 = acc2 + #tostring(x) end
  acc2 = acc2 + v
end
print(acc2)
"####,
    },
    Prog {
        name: "n_retf_types",
        small: true,
        src: r####"
-- the caller's registers have other types than the callee's at the same slot numbers (the types the exit stub fixes belong to the callee's frame)
local function f(n)
  local a, b = n, n + 1
  local s = 0
  for i = 1, n do s = s + i end
  return s + a + b
end
local function caller(r)
  local str = "k" .. (r % 3)
  local fl = r * 0.5
  local tb = {r}
  local v = f(r % 5)
  return #str + fl + tb[1] + v
end
local acc = 0
for r = 1, 3000 do acc = acc + caller(r) end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_upvals",
        small: true,
        src: r####"
-- callee and caller are closures with different upvalues: after the pop the caller's closure (not the callee's) owns the upvalue accesses
local function make(base, mul)
  local cnt = 0
  local function f(n)
    local s = 0
    for i = 1, n * 12 do s = s + i * mul end
    cnt = cnt + 1
    return s
  end
  local function caller(r)
    local v = f(r % 4)
    base = base + 1
    return v + base + cnt * 2
  end
  return caller
end
local c1, c2 = make(1000, 3), make(50000, 7)
local acc = 0
for r = 1, 4000 do acc = acc + c1(r) + c2(r) end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_relink2",
        small: true,
        src: r####"
-- N3b: the caller calls the callee a second time after the pop: the recording meets the callee's traced loop head in a pushed frame (the frame-linking exit =
-- the re-push) and the inline stub rebuilds that frame and jumps into the loop trace
local function f(n)
  local s = 0
  for i = 1, n * 8 do s = s + i end
  return s
end
local w = 0
for r = 1, 400 do w = w + f(r % 9) end -- (the callee's loop gets its own trace first)
print(w)
local function pair(x)
  local a = f(x % 7)
  local b = f((x + 3) % 5)
  return a + b
end
local acc = 0
for r = 1, 6000 do acc = acc + pair(r) end
print(acc)
local function trio(x)
  local a = f(x % 6)
  local b = f((x + 1) % 6)
  local c = f((x + 2) % 6)
  return a * b + c
end
local z = 0
for r = 1, 5000 do z = z + trio(r) end
print(z)
"####,
    },
    Prog {
        name: "n_retf_poly",
        small: true,
        src: r####"
-- the caller's register after the call has a type the callee's register at the same stack position never has in the recording (an int there, a float later):
-- nothing may take the callee's slot types for the caller's
local function f(n)
  local a = n + 1
  local s = 0
  for i = 1, n do s = s + i end
  return s + a
end
local function caller(r, x)
  local p, q, w = x, x + 1, x * 2
  local v = f(r % 5)
  return v + p + q + w
end
local acc = 0
for r = 1, 4000 do acc = acc + caller(r, (r > 2000) and r + 0.5 or r) end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_swap",
        small: true,
        src: r####"
-- a call made by the caller after the pop goes through a callee-identity guard on an upvalue function that changes in the middle of the run
local g = function(x) return x * 2 end
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function pair(x)
  local a = f(x % 7)
  local b = g(a)
  return a + b
end
local acc = 0
for r = 1, 6000 do
  if r == 3000 then g = function(x) return x * 3 + 1 end end
  if r == 4500 then g = function(x) return x - 1 end end
  acc = acc + pair(r)
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_failafter",
        small: true,
        src: r####"
-- what the caller does after the pop cannot be recorded (a three-operand concatenation): the recording ends in failure and the exit must keep the trace
-- that stops before the RETURN
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function caller(r)
  local v = f(r % 6)
  local str = r .. "," .. v .. ";"
  return #str
end
local acc = 0
for r = 1, 5000 do acc = acc + caller(r) end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_hot",
        small: false,
        src: r####"
-- a hot callee loop whose exit is taken often: the side trace that pops and lands in the caller
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local acc = 0
for r = 1, 200000 do
  acc = acc + f(r % 3)
end
print(acc)
"####,
    },
    Prog {
        name: "n_retf_mr_hot",
        small: false,
        src: r####"
-- a hot callee loop whose results go to a multiple-result consumer in the caller (CALL C=0, then SETLIST / RETURN B=0)
local function f(n)
  local s = 0
  for i = 1, n do s = s + i end
  if n % 2 == 0 then return s, n, n + 1 end
  return s, n
end
local function c(n) return {f(n)} end
local function d(n) return f(n) end
local acc = 0
for r = 1, 100000 do
  local t = c(r % 4)
  acc = acc + #t + t[1]
  local a, b, e = d(r % 5)
  acc = acc + a + b + (e or 0)
end
print(acc)
"####,
    },
];
