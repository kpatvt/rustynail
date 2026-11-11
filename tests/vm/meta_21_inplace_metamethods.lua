-- Lua metamethods run in the caller's interpreter loop (vm_unsafe `mm_frame`/`mm_return`):
-- every instruction kind that can call one, results written back, comparisons with both
-- polarities, vararg / tail-calling / multi-result metamethods, errors and tracebacks inside
-- them, yields across them, hooks set inside them and unbounded recursion.
local log = {}
local function L(...) log[#log + 1] = table.concat({...}, " ") end

local mt = {}
local function V(x) return setmetatable({x = x}, mt) end
local function val(o) return type(o) == "table" and o.x or o end
mt.__add = function(a, b) return V(val(a) + val(b)) end
mt.__sub = function(a, b) return V(val(a) - val(b)) end
mt.__mul = function(a, b) return V(val(a) * val(b)) end
mt.__idiv = function(a, b) return V(val(a) // val(b)) end
mt.__band = function(a, b) return V(val(a) & val(b)) end
mt.__shl = function(a, b) return V(val(a) << val(b)) end
mt.__unm = function(a) return V(-a.x) end
mt.__bnot = function(a) return V(~a.x) end
mt.__len = function(a) return a.x * 10 end
mt.__eq = function(a, b) return val(a) == val(b) end
mt.__lt = function(a, b) return val(a) < val(b) end
mt.__le = function(a, b) return val(a) <= val(b) end
mt.__concat = function(a, b) return "(" .. tostring(val(a)) .. ".." .. tostring(val(b)) .. ")" end

local a, b = V(6), V(3)
-- MMBIN / MMBINI / MMBINK, flipped operands
print((a + b).x, (a - 1).x, (1 - a).x, (a * 2.5).x, (2.5 * a).x, (a // b).x, (a & 3).x, (1 << b).x)
print((-a).x, (~a).x, #a, #b)
-- comparisons (registers, immediates with integer/float, flipped), both outcomes
print(a == b, a == V(6), a ~= b, a < b, b < a, a <= V(6), a > b, a >= b)
print(a < 7, a < 6, a <= 6, a > 5, a >= 7, 7 > a, 5.5 < a, a < 6.5, a >= 6.0)
local n = 0
for i = 1, 10 do if V(i) < a then n = n + 1 end; if not (V(i) == a) then n = n + 100 end end
print(n)
-- concatenation: table first, table second, both, inside a longer chain
print(a .. "s", "s" .. a, a .. b, 1 .. a, "x" .. a .. "y" .. b, a .. 2)

-- __index / __newindex functions on every get/set instruction
local store = {}
local p = setmetatable({}, {
  __index = function(t, k) return "get:" .. tostring(k) end,
  __newindex = function(t, k, v) store[#store + 1] = tostring(k) .. "=" .. tostring(v) end,
})
local key, ik = "dyn", 7
local so = setmetatable({}, {__index = function(t, k) return function(self, x) return k .. x end end})
print(p.f, p[key], p[1], p[ik], p[2.5], so:meth(1))
p.f = 1; p[key] = 2; p[3] = 3; p[ik] = "k"; p[true] = false
print(table.concat(store, ","))
_ENV = setmetatable({}, {__index = _G, __newindex = function(t, k, v) rawset(t, k, v .. "!") end})
gv = "set"
print(gv, rawget(_ENV, "gv"))

-- metamethods with varargs, several or no results, tail calls (Lua and C callees)
local q = setmetatable({}, {
  __index = function(...) local t, k = ...; return select("#", ...), k end,
  __add = function(x, y) return end,
  __sub = function(x, y) return string.rep("ab", 2) end,
  __mul = function(x, y) return tostring(y) end,
  __len = function(x) return 1, 2, 3 end,
  __lt = function(x, y) return nil end,
  __le = function(x, y) return 0 end,
})
print(q.k, q + 1, q - 1, q * 5, #q, q < q, q <= q)
local function deep(t, k, d) if d == 0 then return k end return deep(t, k, d - 1) end
local r = setmetatable({}, {__index = function(t, k) return deep(t, k .. "!", 50) end})
print(r.tail, r[r.x])

-- __index metamethods nesting other metamethods (Lua and through C functions)
local cnt = 0
local nest = setmetatable({}, {__index = function(t, k)
  cnt = cnt + 1
  if k > 0 then return t[k - 1] + (V(k) + V(0)).x end
  return 0
end})
print(nest[30], cnt)
local viac = setmetatable({}, {__index = function(t, k)
  return string.gsub("abc", "%w", function(c) return a .. c end)
end})
print(viac.z)

-- errors inside metamethods: message, position, traceback names, recovery
local bad = setmetatable({}, {
  __index = function(t, k) error("idx " .. k) end,
  __add = function(x, y) return x.nope.deeper end,
  __lt = function(x, y) error({code = 42}) end,
})
print(pcall(function() return bad.zz end))
print(pcall(function() return bad + 1 end))
local ok, e = pcall(function() return bad < bad end)
print(ok, type(e), e.code)
local tb = setmetatable({}, {__index = function(t, k) return debug.traceback("tb", 1) end}).x
print((tb:gsub("\n[^\n]*%[C%][^\n]*", ""):gsub("0x%x+", "ADDR")))
print(select(2, xpcall(function() return bad.q end, function(m) return "handled: " .. m end)))
local info = setmetatable({}, {__index = function(t, k) local i = debug.getinfo(1, "n"); return tostring(i.name) .. "/" .. tostring(i.namewhat) end})
print(info.x)

-- yields across metamethods run in place (resumed through finish_op / mm_return)
local ymt = {
  __index = function(t, k) local v = coroutine.yield("idx", k); return v end,
  __newindex = function(t, k, v) coroutine.yield("nidx", k, v); rawset(t, k, v * 2) end,
  __add = function(x, y) return coroutine.yield("add") end,
  __lt = function(x, y) return coroutine.yield("lt") end,
  __eq = function(x, y) return coroutine.yield("eq") end,
  __len = function(x) return coroutine.yield("len") end,
  __concat = function(x, y) return coroutine.yield("cat") end,
  __unm = function(x) return coroutine.yield("unm") end,
}
local co = coroutine.wrap(function()
  local o = setmetatable({}, ymt)
  local o2 = setmetatable({}, ymt)
  local res = {}
  res[#res + 1] = o.field
  o.n = 21
  res[#res + 1] = rawget(o, "n")
  res[#res + 1] = o + 1
  res[#res + 1] = tostring(o < o2)
  res[#res + 1] = tostring(o == o2)
  res[#res + 1] = #o
  res[#res + 1] = o .. "x"
  res[#res + 1] = -o
  if o < 3 then res[#res + 1] = "lt3" else res[#res + 1] = "nlt3" end
  return "done", table.concat(res, ",")
end)
local replies = {"F", nil, "SUM", true, false, 99, "CAT", "NEG", false}
local out, i = {co()}, 0
while out[1] ~= "done" do
  i = i + 1
  log[#log + 1] = table.concat(out, ":", 1, select("#", table.unpack(out)))
  out = {co(replies[i])}
end
print(table.concat(log, " "), out[2])

-- a hook set inside a metamethod, removed by a later one
local lines = 0
local hk = setmetatable({}, {
  __index = function(t, k) debug.sethook(function() lines = lines + 1 end, "l"); return 1 end,
  __add = function(x, y) debug.sethook(); return 2 end,
})
local s = hk.on
s = s + 1
s = s + (hk + 0)
print(s, lines > 0)

-- unbounded recursion through metamethods still reports a C stack overflow
local rec = setmetatable({}, {__index = function(t, k) return t[k + 1] end})
print(pcall(function() return rec[1] end))
local reca = setmetatable({}, {})
getmetatable(reca).__add = function(x, y) return x + y end
print(pcall(function() return reca + 1 end))
local recl
recl = setmetatable({}, {__lt = function(x, y) return x < y end})
print(pcall(function() return recl < recl end))
-- and recovers: metamethods still run after the overflow
print(a + b == V(9), p.after)
