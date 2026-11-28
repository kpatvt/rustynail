-- Section 86.1 (TAILF, `retf` bit 10): side traces that pop their root frame (`RETF`) and then record the caller's `TAILCALL` as a frame
-- replacement. Closure pipelines whose `map` stage does `return f(v)` after its source returned, tail-called functions with more / fewer
-- parameters than arguments, upvalues of the caller and of the callee, several results, a callee that needs a larger frame, errors with levels
-- and tracebacks inside the tail-called function, `debug.getinfo` (`istailcall`) from it, call / return / tail-call hooks installed mid-loop,
-- deep tail recursion after the pop, a callee that changes identity, callees that are not Lua closures.

local function range(a, b) local i = a - 1 return function() i = i + 1 if i <= b then return i end end end
local function map(f, src) return function() local v = src() if v ~= nil then return f(v) end end end
local function map2(f, src) return function() local v = src() if v ~= nil then return f(v, v + 1, v + 2) end end end
local function filter(pred, src)
  return function()
    while true do
      local v = src()
      if v == nil then return nil end
      if pred(v) then return v end
    end
  end
end
local function records(n)
  local i = 0
  return function()
    if i >= n then return nil end
    i = i + 1
    return { id = i, kind = i % 4, value = (i * 7) % 101 }
  end
end
local function fold(f, acc, src) for v in src do acc = f(acc, v) end return acc end
local function add(a, b) return a + b end
local function cnt(n) return n + 1 end

-- 1. the stream_closures shape: records filtered, mapped to a field by a tail call, folded
for round = 1, 3 do
  local out = {}
  for kind = 0, 3 do
    local vals = function() return map(function(r) return r.value end, filter(function(r) return r.kind == kind end, records(3000))) end
    out[#out + 1] = fold(cnt, 0, vals()) .. "/" .. fold(add, 0, vals()) .. "/" .. fold(math.max, 0, vals())
  end
  print("round", round, table.concat(out, " "))
end

-- 2. tail-called functions with fewer / more parameters, several results, upvalues of both sides
local scale = 3
local function two(a, b, c) return a * scale + (b or 0) - (c or 0), a end
local function one(a) return a + scale end
local function many(a, b, c, d, e) return (d == nil and 1 or 0) + (e == nil and 1 or 0) + a + b + c end
for round = 1, 3 do
  local s1 = fold(add, 0, map(one, range(1, 2500)))
  local s2 = fold(add, 0, map2(many, filter(function(x) return x % 3 ~= 0 end, range(1, 2500))))
  local s3, s4 = 0, 0
  for x, y in map2(two, range(1, 2500)) do s3 = s3 + x; s4 = s4 + y end
  scale = scale + 1
  print("shapes", round, s1, s2, s3, s4)
end

-- 3. a callee with a much larger frame than the caller (stack room after the replacement)
local function big(v)
  local a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p = v, v + 1, v + 2, v + 3, v + 4, v + 5, v + 6, v + 7, v + 8, v + 9, v + 10, v + 11, v + 12, v + 13, v + 14, v + 15
  local t = { a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p }
  local s = 0
  for q = 1, #t do s = s + t[q] end
  return s
end
print("big", fold(add, 0, map(big, filter(function(x) return x % 2 == 0 end, range(1, 3000)))))

-- 4. errors inside the tail-called function: levels, tracebacks (the replaced frame is a tail call)
local function boom(r)
  if r.id == 2500 then error("boom at " .. r.id, 1) end
  if r.id == 2700 then error({ code = r.id }) end
  return r.value
end
local function boom2(r)
  if r.id == 2900 then error("level two", 2) end
  return r.value
end
for _, f in ipairs({ boom, boom2 }) do
  local ok, e = pcall(fold, add, 0, map(f, filter(function(r) return r.kind ~= 1 end, records(3000))))
  print("err", ok, type(e) == "table" and e.code or (tostring(e):gsub("^[^:]*:", "")))
end
local tb
local function trace_at(r)
  if r.id == 2222 and not tb then
    tb = debug.traceback("tb", 1)
  end
  return r.value
end
fold(add, 0, map(trace_at, filter(function(r) return r.kind == 2 end, records(3000))))
print("traceback has tail calls", tb ~= nil and tb:find("tail call", 1, true) ~= nil)
local tails, seen = 0, 0
local function info_at(r)
  if r.id % 97 == 0 then
    local ar = debug.getinfo(1, "St")
    seen = seen + 1
    if ar.istailcall then tails = tails + 1 end
  end
  return r.value
end
fold(add, 0, map(info_at, filter(function(r) return r.kind ~= 3 end, records(4000))))
print("istailcall", seen, tails)

-- 5. hooks installed mid-loop: call, return and tail-call events
local events = {}
local function hooked(r)
  if r.id == 2000 then
    debug.sethook(function(ev) events[ev] = (events[ev] or 0) + 1 end, "cr")
  elseif r.id == 2100 then
    debug.sethook()
  end
  return r.value
end
local hs = fold(add, 0, map(hooked, filter(function(r) return r.kind == 0 end, records(3000))))
local evs = {}
for k, v in pairs(events) do evs[#evs + 1] = k .. "=" .. v end
table.sort(evs)
print("hooks", hs, table.concat(evs, " "))

-- 6. deep tail recursion behind the pop
local function down(n, acc) if n == 0 then return acc end return down(n - 1, acc + n) end
local function deep(v) return down(v % 50 + 100, 0) end
print("deep", fold(add, 0, map(deep, filter(function(x) return x % 5 == 1 end, range(1, 4000)))))

-- 7. the tail-called function changes identity (and prototype) mid-stream; callees that are not Lua closures
local which = 0
local fa = function(v) return v * 2 end
local fb = function(v) return v * 3 + 1 end
local function sw(v) which = which + 1; if which % 500 < 250 then return fa(v) else return fb(v) end end
print("switch", fold(add, 0, map(sw, filter(function(x) return x % 4 ~= 0 end, range(1, 4000)))))
local callable = setmetatable({}, { __call = function(_, v) return v + 7 end })
print("callable", fold(add, 0, map(callable, filter(function(x) return x % 3 == 0 end, range(1, 3000)))))
print("cfunc", fold(add, 0, map(math.abs, filter(function(x) return x % 3 == 0 end, range(-1500, 1500)))))
print("tostring", fold(function(a, s) return a + #s end, 0, map(tostring, filter(function(x) return x % 7 == 0 end, range(1, 5000)))))

-- 8. allocation and collections inside the tail-called function
local keep = {}
local function alloc(v)
  local t = { v, v * 2, tag = "x" .. (v % 10) }
  if v % 601 == 0 then keep[#keep + 1] = t; collectgarbage("step") end
  return #t + #t.tag
end
print("alloc", fold(add, 0, map(alloc, filter(function(x) return x % 2 == 1 end, range(1, 6000)))), #keep)

-- 9. the replaced frame is larger than the caller's: values in its high registers must survive collections after exits inside it
local function hold(v)
  local a, b, c, d, e, f, g, h, i, j, k = v, v + 1, v + 2, v + 3, v + 4, v + 5, v + 6, v + 7, v + 8, v + 9, v + 10
  local t = { v, tostring(v) }
  if v % 7 == 0 then
    for q = 1, 30 do local u = { q, q } end
    collectgarbage("step", 1)
  end
  return #t[2] + t[1] + a + k
end
local hsum = 0
for round = 1, 4 do hsum = hsum + fold(add, 0, map(hold, filter(function(x) return x % 2 == 0 end, range(1, 3000)))) end
print("hold", hsum)
