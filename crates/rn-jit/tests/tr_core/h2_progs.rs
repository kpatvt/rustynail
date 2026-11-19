//! H2 programs: `setmetatable` inline in the tier-1 `CALL` / `TAILCALL` templates (`t1inline` row `setmetatable`), the inline tail call of
//! the built-in rows (the frame returns through the `RETURN` that follows the `TAILCALL`), the store of a new key into a table with a free
//! node and the Lua tail call. Each program keeps its hot sites in small functions (so that tier 1 compiles them), calls them with normal
//! and abnormal arguments (the generic path must give the identical result or error message), then changes the situation under the hot
//! sites (a `__gc` field appears in the metatable, the metatable becomes protected, the table is old, a hook is installed).

use super::progs::Prog;

pub const H2_PROGS: &[Prog] = &[
    Prog {
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
    },
    Prog {
        name: "h2_newkey",
        small: true,
        src: r##"
-- H2b: SETFIELD of a key that is not in the table, inline in tier 1 when its main position is free. Generated insertion sequences (so that the
-- same constant keys hit free main positions, collisions, full parts that rehash and nil values), read back in `pairs` order (the node layout
-- decides it) with their sizes; metamethod-absence flags, `__newindex`, old (black) tables, tables of 8+ nodes (layout ids) read by hot loops.
local out = {}
local H_tostring, H_concat, H_pcall, H_type = tostring, table.concat, pcall, type
local function emit(...)
  local t = {}
  for i = 1, select("#", ...) do t[i] = H_tostring((select(i, ...))) end
  out[#out + 1] = H_concat(t, ",")
end
local seed = 20241
local function rnd(n) seed = (seed * 1103515245 + 12345) % 2147483648 return (seed >> 8) % n + 1 end
local keys = {}
for i = 1, 26 do keys[i] = "k" .. i end
keys[#keys + 1] = "__index"; keys[#keys + 1] = "__len"; keys[#keys + 1] = "n"; keys[#keys + 1] = "x"; keys[#keys + 1] = "y"; keys[#keys + 1] = "z"
local function gen(nsets, prefill, withnil, withmt)
  local l = { "local v, MT = ..." }
  local pre = {}
  for j = 1, prefill do pre[#pre + 1] = keys[rnd(#keys)] .. " = " .. j end
  if rnd(4) == 1 then pre[#pre + 1] = "1, 2" end
  l[#l + 1] = "local t = {" .. H_concat(pre, ", ") .. "}"
  if withmt then l[#l + 1] = "setmetatable(t, MT)" end
  for j = 1, nsets do
    local k = keys[rnd(#keys)]
    local r = rnd(8)
    local val = r == 1 and withnil and "nil" or r == 2 and "{}" or r == 3 and "'s" .. j .. "'" or r == 4 and "false" or "v + " .. j
    l[#l + 1] = "t." .. k .. " = " .. val
  end
  l[#l + 1] = "return t"
  return assert(load(H_concat(l, "\n"), "=gen"))
end
local function sig(t)
  local p = {}
  for k, v in pairs(t) do p[#p + 1] = H_tostring(k) .. "=" .. (H_type(v) == "table" and "T" or H_tostring(v)) end
  return H_concat(p, " ") .. " #" .. #t
end
local fns, ni_calls = {}, 0
local MT_NI = { __newindex = function(t, k, v) ni_calls = ni_calls + 1 rawset(t, k, v) end }
local MT_PLAIN = { __index = {} }
for f = 1, 48 do fns[f] = gen(2 + rnd(14), rnd(7) - 1, f % 3 ~= 0, f % 4 ~= 0) end
for f = 1, #fns do
  local fn, last = fns[f], nil
  local sum = 0
  local mt = f % 8 < 4 and MT_NI or MT_PLAIN
  for i = 1, 90 do
    local t = fn(i, mt)
    local s = sig(t)
    if i == 90 then last = s end
    sum = sum + #s
  end
  emit(f, last, sum, ni_calls)
end

-- metamethod-absence flags: the cached "absent" bit of a metatable is invalidated by the insertion of the key
local function set_ni(m, f) m.__newindex = f end
local function set_idx(m, f) m.__index = f end
local function set_len(m, f) m.__len = f end
local function set_eq(m, f) m.__eq = f end
local seen = 0
for i = 1, 120 do
  local m1, m2, m3, m4 = {}, {}, {}, {}
  local o1, o2, o3, o4 = setmetatable({}, m1), setmetatable({}, m2), setmetatable({}, m3), setmetatable({}, m4)
  local o4b = setmetatable({}, m4)
  local _ = o1.zz; o1.zz = 1; local _ = o2.zz; local _ = #o3; local _ = o4 == o4b
  if i > 60 then
    set_ni(m1, function(t, k, v) seen = seen + 1 rawset(t, k, v) end)
    set_idx(m2, function(t, k) seen = seen + 10 return k end)
    set_len(m3, function() seen = seen + 100 return 7 end)
    set_eq(m4, function() seen = seen + 1000 return true end)
    o1.qq = 5
    local r2 = o2.nokey
    local r3 = #o3
    local r4 = o4 == o4b
    if i % 20 == 0 then emit("flags", seen, r2, r3, r4, rawget(o1, "qq")) end
  end
end
emit("flags", seen)

-- `__newindex` handlers see the new key (the inline path must not be taken)
local log = {}
local function mk_h() return setmetatable({}, { __newindex = function(t, k, v) log[#log + 1] = k rawset(t, k, v) end }) end
local function put3(t, v) t.a = v; t.b = v; t.c = v; t.a = v + 1 return t end
for i = 1, 100 do local t = put3(mk_h(), i) if i % 50 == 0 then emit("ni", H_concat(log, ""), t.a, t.b, t.c) end end
local proxy = setmetatable({}, { __newindex = {} })
local function put_into(t, v) t.k = v end
for i = 1, 100 do put_into({}, i) put_into(proxy, i) end
emit("proxy", rawget(proxy, "k"), getmetatable(proxy).__newindex.k)

-- nil values: no key is created; a following insertion behaves as in the interpreter (node reuse / rehash timing, shown by `pairs`)
local function nil_then(t, v) t.x = nil; t.y = v; t.z = nil; t.n = v + 1; t.k1 = v + 2; t.k2 = nil; t.k3 = v + 3 return t end
local last
for i = 1, 100 do last = sig(nil_then({ k4 = 1 }, i)) end
emit("nil", last)
local function nil_fill(t, v) t.k1 = v; t.k2 = nil; t.k3 = nil; t.k5 = v; t.k6 = nil; t.k7 = v; t.k8 = v; t.k9 = v return t end
for i = 1, 100 do last = sig(nil_fill({ a = 1, b = 2, c = 3 }, i)) end
emit("nil2", last)

-- old (black) tables: an insertion of a young value needs the collector's barrier (generational and incremental)
collectgarbage("generational")
local olds = {}
for i = 1, 300 do olds[i] = {} end
collectgarbage(); collectgarbage()
local function put_young(t, i) t.v = { i } end
for i = 1, 300 do
  put_young(olds[i], i)
  if i % 30 == 0 then collectgarbage("step") end
end
collectgarbage("step")
local junk = {}
for i = 1, 900 do junk[i] = { -i } end
local bad = 0
for i = 1, 300 do if olds[i].v[1] ~= i then bad = bad + 1 end end
emit("gen-black", bad)
collectgarbage("incremental")
local inc = {}
for i = 1, 600 do inc[i] = {} end
collectgarbage(); collectgarbage()
collectgarbage("step", 0)
for i = 1, 600 do
  put_young(inc[i], i)
  collectgarbage("step", 0)
end
local junk2 = {}
for i = 1, 900 do junk2[i] = { -i } end
collectgarbage()
bad = 0
for i = 1, 600 do if inc[i].v[1] ~= i then bad = bad + 1 end end
emit("inc-black", bad)

-- 8+ node parts: the layout id changes with each insertion; hot loops read the keys through shape guards
local function mk8(i) local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7 } t.h = i t.i = i + 1 return t end
local function mk8b(i) local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7 } t.i = i + 1 t.h = i t.j = i + 2 return t end
local function mk16(i) local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9 } t.j = i t.k = i + 1 t.l = i + 2 t.m = i + 3 return t end
local function rd(t) return t.a + t.b + t.c + t.d + t.e + t.f + t.g + (t.h or 0) + (t.i or 0) + (t.j or 0) + (t.k or 0) + (t.l or 0) + (t.m or 0) end
local function grow(t, i) t.h = i t.j = i + 1 t.m = i + 2 return t end
local shapes = { mk8, mk8b, mk16 }
local acc = 0
for i = 1, 400 do
  local t = shapes[i % 3 + 1](i)
  acc = acc + rd(t)
  local u = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7 }
  acc = acc + rd(u)
  grow(u, i)
  acc = acc + rd(u)
end
emit("layout", acc)
local pool = {}
for i = 1, 64 do pool[i] = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7 } end
for round = 1, 6 do
  local s = 0
  for i = 1, 64 do s = s + rd(pool[i]) end
  for i = 1, 64 do grow(pool[i], i + round) end
  emit("pool", round, s)
  for i = 1, 64 do pool[i] = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7 } end
end

-- a hook and an error inside the store path (hooks exit before the store; a protected `__newindex` raising)
local function bad_put(t) t.q = 1 end
local E = setmetatable({}, { __newindex = function() error("no") end })
for i = 1, 100 do local ok, e = H_pcall(bad_put, E) if i == 100 then emit("err", ok, (tostring(e):gsub("^[^:]*:%d+: ", ""))) end end
local cnt = 0
debug.sethook(function() cnt = cnt + 1 end, "l")
for i = 1, 10 do put_young({}, i) end
debug.sethook()
emit("hook", cnt > 0)
print(H_concat(out, "\n"))
"##,
    },
    Prog {
        name: "h2_tail",
        small: true,
        src: r##"
-- H2c: TAILCALL with B = 0 (the last argument is a call, so the argument count is read from `top`) of a Lua closure, inline in tier 1.
local out = {}
local H_tostring, H_concat, H_pcall, H_select, H_gsub = tostring, table.concat, pcall, select, string.gsub
local function emit(...)
  local t = {}
  for i = 1, H_select("#", ...) do t[i] = H_gsub(H_tostring((H_select(i, ...))), "0x%x+", "0xADDR") end
  out[#out + 1] = H_concat(t, ",")
end

local function none() end
local function one(x) return x end
local function three(x) return x, x + 1, x + 2 end
local function sum3(a, b, c) return (a or 0) + (b or 0) * 10 + (c or 0) * 100 end
local function sum5(a, b, c, d, e) return (a or 0) + (b or 0) * 10 + (c or 0) * 100 + (d or 0) * 1000 + (e or 0) * 10000 end
local function va(...) return select("#", ...), ... end
local function id2(a, b) return a, b end

-- the sites: argument counts 0, 1, 3 of the last call; callee with fewer / equal / more parameters; vararg callee
local function t0(x) return sum3(x, none()) end
local function t1(x) return sum3(x, one(x + 1)) end
local function t3(x) return sum3(x, three(x)) end
local function t5(x) return sum5(x, three(x)) end
local function t6(x) return sum5(x, x, three(x)) end
local function tv(x) return va(x, three(x)) end
local function tm(x) return id2(three(x)) end
local function tn(x) return id2(none()) end
local function tz() return three(one(5)) end
local function tdeep(n, x) if n == 0 then return three(x) end return tdeep(n - 1, one(x + 1)) end
local function tsel(x) return select("#", three(x)) end

local acc = 0
for i = 1, 300 do
  acc = acc + t0(i) + t1(i) + t3(i) + t5(i) + t6(i) + (tv(i)) + (tm(i)) + (tn(i) or 0) + tz()
  local a, b, c = tdeep(40, i)
  acc = acc + a + b + c + tsel(i)
  if i % 100 == 0 then emit(i, acc, t0(i), t1(i), t3(i), t5(i), t6(i), tv(i)) emit(tm(i)) emit(tn(i)) emit(tz()) emit(a, b, c) end
end

-- a callee that is not a Lua closure, a callee replaced under the site, a callable table
local function tnat(x) return math.max(x, one(x + 1)) end
local function tnat2(x) return tostring(one(x)) end
local calls = 0
local cobj = setmetatable({}, { __call = function(self, a, b, c) calls = calls + 1 return a, b, c end })
local function tcall(x) return cobj(x, three(x)) end
local function tsw(f, x) return f(x, three(x)) end
for i = 1, 300 do
  acc = acc + tnat(i) + #tnat2(i)
  local a, b, c = tcall(i)
  acc = acc + a + b + c
  acc = acc + tsw(sum3, i) + tsw(sum5, i) + tsw(math.max, i) + select("#", tsw(va, i)) + select("#", tsw(id2, i))
  if i % 100 == 0 then emit(acc, calls) end
end

-- closures capturing locals (`k`): the open upvalues are closed by the tail call
local function mkc(x)
  local cap = x
  local f = function() cap = cap + 1 return cap end
  if x % 2 == 0 then return sum3(f(), one(x)) end
  return sum3(x, three(f()))
end
for i = 1, 300 do acc = acc + mkc(i) end
emit("k", acc)
local fns = {}
local function keep(x)
  local cap = x
  fns[#fns + 1] = function() cap = cap + 1 return cap end
  return sum3(x, three(cap))
end
for i = 1, 100 do keep(i) end
emit("k2", fns[1](), fns[1](), fns[100]())

-- errors, hooks, tracebacks through the tail call
local function fail(a, b, c) error("boom " .. tostring(a) .. tostring(c)) end
local function tfail(x) return fail(x, three(x)) end
for i = 1, 100 do local ok, e = H_pcall(tfail, i) if i == 100 then emit("err", ok, (H_gsub(e, "^[^:]*:%d+: ", ""))) end end
local function tb(a, b, c) return debug.traceback("tb", 1) end
local function ttb(x) return tb(x, three(x)) end
for i = 1, 50 do ttb(i) end
emit("tb", (H_gsub(ttb(1), "[^\n]*:%d+:", "L:")))
local hooked = 0
debug.sethook(function() hooked = hooked + 1 end, "cr")
for i = 1, 10 do t3(i) end
debug.sethook()
emit("hook", hooked > 0)

-- coroutines and deep recursion (the stack moves)
local co = coroutine.wrap(function(x) while true do x = coroutine.yield(t3(x), tdeep(30, x)) end end)
local cacc = 0
for i = 1, 200 do cacc = cacc + co(i) end
emit("co", cacc)
local function rec(n, a, b, c) if n == 0 then return a + b + c end return rec(n - 1, three(a + n)) end
emit("rec", rec(500, 1, 2, 3), rec(3000, 1, 2, 3))
local function nilfill(a, b, c, d) return tostring(a) .. tostring(b) .. tostring(c) .. tostring(d) end
local function tnil(x) return nilfill(x, one(x)) end
for i = 1, 300 do tnil(i) end
emit("nilfill", tnil(7), nilfill(none()))
print(H_concat(out, "\n"))
"##,
    },
];
