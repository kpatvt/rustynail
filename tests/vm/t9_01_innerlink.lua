-- docs/JIT_TRACE.md section 47 (T9, `innerlink`): a root recording of an outer loop that reaches the counted loop of a callee (data-dependent bounds)
-- whose loop head has a root trace links into it with the frame-linking exit; the side traces of the inner loop's exit pop into the caller (RETF)
-- and link back to the outer loop. Same output as the reference in every mode.

-- (1) the deltablue shape: an outer loop over objects, a method with a counted loop over a per-object array (lengths 0..7), a store after the call
local Var = {}
Var.__index = Var
local function newVar(i)
  local v = setmetatable({ id = i, cons = {}, mark = 0 }, Var)
  for k = 1, i % 8 do v.cons[k] = { w = k * 3 + i, sat = (k + i) % 3 ~= 0 } end
  return v
end
function Var:consuming(coll)
  local cc = self.cons
  for i = 1, #cc do
    local c = cc[i]
    if c.sat then coll[#coll + 1] = c.w end
  end
end
local vars = {}
for i = 1, 60 do vars[i] = newVar(i) end
local total = 0
for round = 1, 40 do
  local coll = {}
  for i = 1, #vars do
    local v = vars[i]
    v:consuming(coll)
    v.mark = v.mark + #coll % 5
  end
  total = total + #coll
end
local marks = 0
for i = 1, #vars do marks = marks + vars[i].mark end
print("deltablue shape", total, marks)

-- (2) the outer is a function (the call is outside every loop of it), called from a loop: the inner exit pops into it and runs to its RETURN
local function sum_range(t, lo, hi)
  local s = 0
  for i = lo, hi do s = s + t[i] end
  return s
end
local data = {}
for i = 1, 64 do data[i] = (i * 7) % 13 end
local function window(t, at)
  local w = at % 9
  local s = sum_range(t, at, at + w)
  local d = s * 2 - w
  return d + at
end
local acc = 0
for r = 1, 30 do
  for at = 1, 50 do acc = acc + window(data, at) end
end
print("fn outer", acc)

-- (3) steps other than 1, negative and computed steps, zero-trip inner loops
local function stepped(n, st)
  local s = 0
  if st > 0 then
    for i = 1, n, st do s = s + i end
  else
    for i = n, 1, st do s = s - i end
  end
  return s
end
local sacc = 0
for r = 1, 25 do
  for n = 0, 30 do
    sacc = sacc + stepped(n, (n % 4) + 1) + stepped(n, -((n % 3) + 1))
  end
end
print("steps", sacc)

-- (4) an error raised inside the inner loop (after the link) and one raised in the caller after the pop, under pcall
local function find_bad(t, limit)
  for i = 1, #t do
    if t[i] > limit then error("bad value " .. t[i] .. " at " .. i) end
  end
  return #t
end
local function scan_all(lists, limit, fail_after)
  local n = 0
  for j = 1, #lists do
    n = n + find_bad(lists[j], limit)
    if n > fail_after then error("caller limit " .. n) end
  end
  return n
end
local lists = {}
for j = 1, 40 do
  local l = {}
  for k = 1, (j % 6) + 1 do l[k] = (j * k) % 17 end
  lists[j] = l
end
for _, c in ipairs({ { 100, 1e9 }, { 100, 1e9 }, { 15, 1e9 }, { 100, 50 }, { 100, 1e9 }, { 16, 1e9 } }) do
  for _ = 1, 6 do
    local ok, e = pcall(scan_all, lists, c[1], c[2])
    if _ == 6 then print("errors", c[1], c[2], ok, (tostring(e):gsub("^.-:%d+: ", ""))) end
  end
end

-- (5) the inner loop yields from a coroutine (the frames below the link are a coroutine's), and the outer loop resumes
local function producer(lists2)
  for j = 1, #lists2 do
    local l = lists2[j]
    local s = 0
    for k = 1, #l do
      s = s + l[k]
      if s % 11 == 0 then coroutine.yield(j, s) end
    end
  end
  return "done"
end
local ys = 0
for r = 1, 8 do
  local co = coroutine.wrap(producer)
  while true do
    local a, b = co(lists)
    if a == "done" then break end
    ys = ys + a + b
  end
end
print("coroutine", ys)

-- (6) a count hook installed between rounds and from inside the inner loop, removed again
local hookn = 0
local function hooked_sum(t)
  local s = 0
  for i = 1, #t do
    s = s + t[i]
    if s == 23 and hookn == 0 then debug.sethook(function() hookn = hookn + 1 end, "", 100) end
  end
  return s
end
local hs = 0
for r = 1, 30 do
  if r == 20 then debug.sethook(function() hookn = hookn + 1 end, "", 1000) end
  if r == 25 then debug.sethook() end
  for j = 1, #lists do hs = hs + hooked_sum(lists[j]) end
end
debug.sethook()
print("hooks", hs, hookn > 0)

-- (7) the stack grows inside the inner loop (a deep recursion called from its body), collections in the continuation
local function deep(n) if n == 0 then return 0 end return 1 + deep(n - 1) end
local function grow(t, d)
  local s = 0
  for i = 1, #t do
    s = s + t[i]
    if i == #t and d % 7 == 0 then s = s + deep(150 + d) end
  end
  return s
end
local gs, junk = 0, {}
for r = 1, 20 do
  for j = 1, #lists do
    gs = gs + grow(lists[j], r * j)
    junk[#junk + 1] = { gs }
    if #junk > 200 then junk = {} collectgarbage("step") end
  end
end
print("grow", gs)

-- (8) a recursive builder whose loop calls itself (the storage shape: the outer and the inner loop are the same prototype)
local count
local function build(depth, seed)
  count = count + 1
  if depth == 1 then
    local a = {}
    for i = 1, seed % 10 + 1 do a[i] = false end
    return a
  end
  local arr = {}
  for i = 1, 4 do arr[i] = build(depth - 1, seed * 31 + i) end
  return arr
end
local bt = 0
for r = 1, 12 do
  count = 0
  build(5, r)
  bt = bt + count
end
print("recursive", bt)

-- (9) the inner loop is a metamethod's (not linked) and a vararg callee's (not linked): plain results
local V = {}
V.__index = V
V.__add = function(a, b)
  local r = setmetatable({}, V)
  for i = 1, math.max(#a, #b) do r[i] = (a[i] or 0) + (b[i] or 0) end
  return r
end
local function mk(n) local r = setmetatable({}, V) for i = 1, n do r[i] = i end return r end
local function vsum(...)
  local s = 0
  for i = 1, select("#", ...) do s = s + (select(i, ...)) end
  return s
end
local vs, vv = 0, mk(1)
for r = 1, 200 do
  vv = vv + mk(r % 5 + 1)
  vs = vs + vsum(r, r % 3, r % 7)
end
print("mm vararg", #vv, vv[1], vs)

-- (10) the callee's loop runs on in the interpreter-visible frames: debug.traceback from inside the linked inner loop
local tb
local function tracer(t)
  for i = 1, #t do
    if t[i] == 13 and not tb then tb = debug.traceback("tb", 1) end
  end
end
for r = 1, 20 do
  for j = 1, #lists do tracer(lists[j]) end
end
local nlines = select(2, (tb or ""):gsub("\n", "\n"))
print("traceback", tb ~= nil, (tb or ""):match("in %a+ '?(%a+)") or "?", nlines >= 3)

-- (11) the inner loop's exits fail first (its callers' loops are not traced: a C call the recorder does not take), then an outer loop that links
-- into it appears: the exits blacklisted meanwhile try again and pop into the new caller
local function weigh(t)
  local s = 0
  for i = 1, #t do s = s + t[i] * i end
  return s
end
local w1 = 0
for r = 1, 400 do
  local l = lists[r % #lists + 1]
  w1 = w1 + weigh(l) + #string.format("%q", "x" .. r % 3)
end
local w2 = 0
for r = 1, 30 do
  for j = 1, #lists do
    local l = lists[j]
    w2 = w2 + weigh(l)
    l.last = w2 % 7
  end
end
print("woken", w1, w2)

-- (12) a function whose function-entry trace links into its callee's loop (its own callers' loops are not traced): the inner exit pops into it
-- outside every loop and runs to its RETURN
local function span(t, k)
  local s = 0
  for i = 1, k do s = s + (t[i] or 1) end
  return s
end
local function shaped(t, k)
  local a = span(t, k % 6)
  local b = a * 3 - k
  return b + #t
end
local long = {}
for i = 1, 300 do long[i] = i % 4 end
local sh = span(long, 300) + span(long, 299) -- (the callee's loop is traced first)
for r = 1, 600 do
  sh = sh + shaped(lists[r % #lists + 1], r) + #string.format("%q", "y" .. r % 5)
end
print("fn entry", sh)

-- (13) the shortest continuation: the caller adds the result and loops
local function tally(t)
  local s = 0
  for i = 1, #t do s = s + t[i] end
  return s
end
local ts = tally(long) + tally(long) -- (the callee's loop is traced first)
for r = 1, 40 do
  for j = 1, #lists do ts = ts + tally(lists[j]) end
end
print("short", ts)
