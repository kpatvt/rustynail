//! H2 programs: `setmetatable` inline in the tier-1 `CALL` / `TAILCALL` templates (`t1inline` row `setmetatable`), the inline tail call of
//! the built-in rows (the frame returns through the `RETURN` that follows the `TAILCALL`), the store of a new key into a table with a free
//! node and the Lua tail call. Each program keeps its hot sites in small functions (so that tier 1 compiles them), calls them with normal
//! and abnormal arguments (the generic path must give the identical result or error message), then changes the situation under the hot
//! sites (a `__gc` field appears in the metatable, the metatable becomes protected, the table is old, a hook is installed).

use super::progs::Prog;

pub const H2_PROGS: &[Prog] = &[Prog {
    name: "h2_setmt",
    small: true,
    src: r##"
local N = 300
local out = {}
local V = {}
V.__index = V
function V.get(self) return self.x end
local H_pcall, H_tostring, H_select, H_concat, H_type, H_gsub = pcall, tostring, select, table.concat, type, string.gsub
local function emit(...)
  local t = {}
  for i = 1, H_select("#", ...) do t[i] = H_gsub(H_tostring((H_select(i, ...))), "0x%x+", "0xADDR") end
  out[#out + 1] = H_concat(t, ",")
end

-- the sites: expression (C = 2), statement (C = 1), multiple results (C = 0), tail call, tail call in a vararg function
local function s_expr(t, m) local r = setmetatable(t, m) return r end
local function s_stmt(t, m) setmetatable(t, m) return t end
local function s_multi(t, m) local r = { setmetatable(t, m) } return r[1], #r end
local function s_tail(t, m) return setmetatable(t, m) end
local function s_vtail(t, m, ...) return setmetatable(t, m) end
local function s_ctor(x) return setmetatable({ x = x, y = x + 1 }, V) end
local function s_one(t) return setmetatable(t) end
local function s_wrap(t, m) return (setmetatable(t, m)) end

-- 1. normal use, every site
for i = 1, N do
  local a = s_expr({ x = i }, V)
  local b = s_stmt({ x = i + 1 }, V)
  local c, n = s_multi({ x = i + 2 }, V)
  local d = s_tail({ x = i + 3 }, V)
  local e = s_vtail({ x = i + 4 }, V, 1, 2, 3)
  local f = s_ctor(i + 5)
  local g = s_wrap({ x = i + 6 }, V)
  if i % 100 == 1 or i == N then
    emit(i, a:get(), b:get(), c:get(), n, d:get(), e:get(), f:get(), f.y, g:get(), getmetatable(a) == V, getmetatable(e) == V)
  end
end

-- 2. abnormal arguments at the hot sites: the generic path raises the errors
local function try(f, ...) local ok, e = H_pcall(f, ...) emit("try", ok, e) end
for i = 1, 40 do
  if i % 10 == 0 then
    try(s_expr, 1, V)
    try(s_expr, {}, 5)
    try(s_expr, {}, false)
    try(s_expr, {}, "x")
    try(s_stmt, {})
    try(s_tail, "t", V)
    try(s_tail, {}, 2.5)
    try(s_one, {})
    try(s_multi, nil, nil)
  else
    emit(getmetatable(s_expr({}, nil)), getmetatable(s_tail({}, nil)), getmetatable(s_stmt({}, V)) == V)
  end
end

-- 3. old metatable and a protected one (`__metatable`)
local P = { __metatable = "locked", __index = V }
local Q = {}
local function replace(t, m) return setmetatable(t, m) end
for i = 1, N do
  local t = setmetatable({}, Q)
  local u = replace(t, V)
  local w = setmetatable({}, P)
  if i % 100 == 0 then
    emit(getmetatable(u) == V, getmetatable(w))
    try(replace, w, V)
    try(s_tail, w, V)
    try(s_stmt, w, Q)
  end
end

-- 4. a `__gc` field appears in a metatable the hot site has seen without one: the finalizers must run
local fired = 0
local G = {}
for i = 1, N do local o = s_expr({}, G) end
G.__gc = function() fired = fired + 1 end
for i = 1, 60 do s_expr({}, G); s_tail({}, G); s_stmt({}, G) end
collectgarbage(); collectgarbage()
emit("gc", fired)
local G2 = { __gc = false }
collectgarbage("stop")
for i = 1, 100 do s_expr({}, G2) end
G2.__gc = function() fired = fired + 1 end
for i = 1, 20 do s_tail({}, G2) end
collectgarbage("restart")
collectgarbage(); collectgarbage()
emit("gc2", fired)

-- 5. tables that are old or black when the metatable is stored, metatables that are fresh (the collector's barrier)
local keep, bad = {}, 0
for i = 1, 400 do
  local t = {}
  keep[i % 37 + 1] = t
  if i % 3 == 0 then collectgarbage("step", 1) end
  local m = { __index = { v = i } }
  if i % 2 == 0 then s_stmt(t, m) else s_tail(t, m) end
  local junk = { i, i, i }
  if i % 7 == 0 then collectgarbage("step", 1) end
  if t.v ~= i then bad = bad + 1 end
end
collectgarbage()
emit("barrier", bad)
for i = 1, 37 do local t = keep[i] if t and t.v == nil then bad = bad + 1 end end
emit("barrier2", bad)

-- 5b. generational mode: old (black) tables get a young metatable; a young collection must keep it (the barrier of `setmetatable`)
collectgarbage("generational")
local olds = {}
for i = 1, 300 do olds[i] = {} end
collectgarbage(); collectgarbage()
for i = 1, 300 do
  local m = { __index = { v = i } }
  setmetatable({}, m) -- (a throw-away use caches "no __gc" in `m`: the inline store needs the cached bit)
  if i % 2 == 0 then s_stmt(olds[i], m) else s_tail(olds[i], m) end
  if i % 30 == 0 then collectgarbage("step") end -- (a C call the trace recorder does not take: the loop stays in tier 1)
end
collectgarbage("step")
collectgarbage("step")
local junk = {}
for i = 1, 900 do junk[i] = { __index = { v = -i } } end
local bad2 = 0
for i = 1, 300 do if olds[i].v ~= i then bad2 = bad2 + 1 end end
emit("gen-barrier", bad2)
collectgarbage("incremental")

-- 5c. incremental mode: the tables are marked black by the steps of a running cycle, the metatables are fresh (white)
collectgarbage("incremental")
local inc = {}
for i = 1, 600 do inc[i] = {} end
collectgarbage(); collectgarbage()
collectgarbage("step", 0)
for i = 1, 600 do
  local m = { __index = { v = i + 1 } }
  setmetatable({}, m)
  if i % 2 == 0 then s_stmt(inc[i], m) else s_tail(inc[i], m) end
  collectgarbage("step", 0) -- (a C call the trace recorder does not take: the loop stays in tier 1)
end
local junk2 = {}
for i = 1, 900 do junk2[i] = { __index = { v = -i } } end
collectgarbage()
local bad3 = 0
for i = 1, 600 do if inc[i].v ~= i + 1 then bad3 = bad3 + 1 end end
emit("inc-barrier", bad3)

-- 6. in a metamethod frame (`__index` function and `__eq` / `__lt`-style frames of tier 1) that tail-calls `setmetatable`
local mmv = setmetatable({}, { __index = function(t, k) return setmetatable({ k = k }, V) end })
local cmpm = { __lt = function(a, b) return s_tail({ r = 1 }, V) ~= nil end, __le = function(a, b) return s_tail({ r = 2 }, nil) == nil end,
  __add = function(a, b) return setmetatable({ x = 7 }, V) end }
local ca, cb = setmetatable({}, cmpm), setmetatable({}, cmpm)
local acc = 0
for i = 1, N do
  local r = mmv[i]
  if getmetatable(r) == V and r.k == i then acc = acc + 1 end
  if ca < cb then acc = acc + 1 end
  if ca <= cb then acc = acc + 1 end
  local q = ca + cb
  if getmetatable(q) == V then acc = acc + 1 end
end
emit("mm", acc)

-- 7. the call replaced under the sites: another function (not the built-in), the global shadowed, a hook
local real = setmetatable
local function s_glob(t, m) return setmetatable(t, m) end
local calls = 0
for i = 1, N do
  if i == 200 then setmetatable = function(t, m) calls = calls + 1 return real(t, m) end end
  if i == 250 then setmetatable = real end
  local r = s_glob({ x = i }, V)
  if i % 100 == 0 then emit("glob", i, calls, r:get()) end
end
emit("glob", calls)
local hooked = 0
local function s_hook(t, m) return setmetatable(t, m) end
for i = 1, N do s_hook({}, V) end
debug.sethook(function() hooked = hooked + 1 end, "c")
for i = 1, 20 do s_hook({}, V) end
debug.sethook()
emit("hook", hooked > 0)
local cnt = 0
debug.sethook(function() cnt = cnt + 1 end, "", 7)
for i = 1, 100 do s_tail({}, V); s_stmt({}, V) end
debug.sethook()
emit("count", cnt > 0)

-- 8. tail call of the other inline built-ins (the frame returns through the RETURN after the TAILCALL), with abnormal arguments
local function t_type(v) return type(v) end
local function t_floor(v) return math.floor(v) end
local function t_abs(v) return math.abs(v) end
local function t_max(a, b) return math.max(a, b) end
local function t_min(a, b) return math.min(a, b) end
local function t_sqrt(v) return math.sqrt(v) end
local function t_byte(s, i) return string.byte(s, i) end
local function t_len(s) return string.len(s) end
local function t_rawlen(t) return rawlen(t) end
local function t_sel(...) return select("#", ...) end
local function t_vtype(v, ...) return type(v) end
local function t_vfloor(v, ...) return math.floor(v) end
local function t_none() return type() end
local function t_bad(v) return math.floor(v) end
local tacc = {}
for i = 1, N do
  local f = i + 0.5
  tacc[1] = (tacc[1] or 0) + #t_type(i) + t_floor(f) + t_abs(-i) + t_max(i, 3) + t_min(i, 3) + t_len("abc") + t_rawlen({ 1, 2 })
  tacc[2] = (tacc[2] or 0) + t_sqrt(i * i) + t_byte("abc", 2) + t_sel(1, 2, 3) + #t_vtype(f, 1, 2) + t_vfloor(f, 1)
end
emit("tail", tacc[1], tacc[2])
for i = 1, 30 do
  if i % 10 == 0 then
    try(t_none)
    try(t_bad, "x")
    try(t_bad, {})
    try(t_floor, nil)
    emit(t_floor(2 ^ 70), t_floor(-0.0), t_abs(-0.0), t_min(-0.0, 0.0), t_max(0.0, -0.0))
  end
end

-- 9. the result of a native tail call goes to the caller's wanted count (0, 1, many, table constructor, call argument, pcall)
local function ret_tail_gen(t) return setmetatable(t, V) end
local function ret_vtail(t, ...) return setmetatable(t, V) end
local many = 0
for i = 1, N do
  local a, b, c = ret_tail_gen({ x = 1 })
  if a and b == nil and c == nil then many = many + 1 end
  local t = { ret_tail_gen({ x = 2 }), ret_vtail({ x = 3 }, 9, 9) }
  if #t == 2 then many = many + 1 end
  local ok, r = H_pcall(ret_tail_gen, { x = 4 })
  if ok and r.x == 4 then many = many + 1 end
  ret_tail_gen({})
  H_select("#", ret_tail_gen({}))
end
emit("many", many)

-- 10. in a coroutine, under pcall, and with the stack moving (deep recursion)
local co = coroutine.wrap(function(t) for i = 1, N do t = coroutine.yield(s_tail({ x = i }, V)) end return "done" end)
local cacc = 0
for i = 1, N do local r = co({}) if H_type(r) == "table" and r:get() == i then cacc = cacc + 1 end end
emit("co", cacc)
local function deep(n) if n == 0 then return s_tail({ x = 0 }, V) end local r = deep(n - 1) return s_tail(r, V) end
emit("deep", getmetatable(deep(200)) == V)
-- 11. weak tables made through the sites (`__mode` is read by the collector at traversal)
local WV, WK = { __mode = "v" }, { __mode = "k" }
local wv, wk = {}, {}
for i = 1, N do wv[i] = s_expr({}, WV); wv[i][1] = {}; wv[i].keep = V end
local ws = s_tail({}, WV)
for i = 1, 50 do ws[i] = {} end
local wkt = s_stmt({}, WK)
for i = 1, 50 do wkt[{}] = i end
collectgarbage(); collectgarbage()
local nw, nk = 0, 0
for _ in pairs(ws) do nw = nw + 1 end
for _ in pairs(wkt) do nk = nk + 1 end
emit("weak", nw, nk, #wv, getmetatable(ws) == WV)
print(H_concat(out, "\n"))
"##,
}];
