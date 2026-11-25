-- Section 59 M1 (`tmovepre`: table.move into an empty destination sizes its array first) and R1 (`tpop`: table.remove(t) of a table without
-- a metatable): results, borders, traversal order, memory and metamethods must be those of the element-by-element library code.
local move, remove, insert = table.move, table.remove, table.insert

local function show(t, n)
  local o = {}
  for i = 0, n do o[#o + 1] = tostring(t[i]) end
  return table.concat(o, ",")
end
local function keys(t)
  local ks = {}
  for k, v in pairs(t) do ks[#ks + 1] = tostring(k) .. "=" .. tostring(v) end
  return table.concat(ks, " ")
end

-- the bytes a construction leaves allocated (collector stopped)
local mt0 = {}
local function cost(mk)
  collectgarbage()
  collectgarbage("stop")
  local c0 = collectgarbage("count")
  local d = mk()
  local c1 = collectgarbage("count")
  collectgarbage("restart")
  return (c1 - c0) * 1024, d
end

-- M1: dense sources of every size class, into {} and into a destination that the fast path must not touch (a metatable without
-- __newindex: the same raw stores, the same growth)
for _, n in ipairs({ 1, 2, 3, 4, 5, 7, 8, 9, 16, 17, 33, 50, 64, 100 }) do
  local src = {}
  for i = 1, n do src[i] = i * 3 end
  local c1, d1 = cost(function() return move(src, 1, n, 1, {}) end)
  local c2, d2 = cost(function() return move(src, 1, n, 1, setmetatable({}, mt0)) end)
  d1[n + 1] = "x"
  d2[n + 1] = "x"
  local c3 = cost(function() d1[n + 2] = 1; d1[n + 3] = 2 end)
  local c4 = cost(function() d2[n + 2] = 1; d2[n + 3] = 2 end)
  print("dense", n, #d1, #d2, c1 == c2, c3 == c4, d1[n], d1[1], keys(d1) == keys(d2))
end

-- M1: sources with holes (nil values are not inserted: the growth differs from the presized array)
local holes = {
  { 1, nil, 3 },
  { 1, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil, 12 },
  { nil, 2, 3, 4, 5, 6, 7, 8, 9 },
  { 1, 2, 3, 4, 5, 6, 7, nil },
  { 1, 2, 3, nil, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17 },
}
for k, src in ipairs(holes) do
  local n = 17
  if k <= 4 then n = ({ 3, 12, 9, 8 })[k] end
  local c1, d1 = cost(function() return move(src, 1, n, 1, {}) end)
  local c2, d2 = cost(function() return move(src, 1, n, 1, setmetatable({}, mt0)) end)
  print("holes", k, #d1, #d2, c1 == c2, keys(d1), keys(d1) == keys(d2))
  d1[2] = "a"; d2[2] = "a"
  d1[n + 1] = "b"; d2[n + 1] = "b"
  print("holes+", k, #d1, #d2, keys(d1) == keys(d2))
end

-- M1: destination offsets other than 1, sub-ranges, a source range partly in the hash part
local src = {}
for i = 1, 20 do src[i] = i end
src[25] = 25
for _, a in ipairs({ { 1, 20, 2 }, { 3, 10, 1 }, { 5, 5, 1 }, { 1, 25, 1 }, { 18, 25, 1 }, { 20, 26, 1 }, { 1, 8, 9 } }) do
  local d = move(src, a[1], a[2], a[3], {})
  print("range", a[1], a[2], a[3], #d, show(d, 12), keys(d) == keys(move(src, a[1], a[2], a[3], setmetatable({}, mt0))))
end
-- non-empty destinations, the same table, an empty range
local d = { 9, 9 }
print("nonempty", show(move(src, 1, 5, 1, d), 6), #d)
local h = { x = 1 }
print("nonempty-hash", show(move(src, 1, 5, 1, h), 6), #h, h.x)
local same = { 1, 2, 3, 4 }
print("same", show(move(same, 1, 4, 1), 5), show(move(same, 2, 4, 1), 5))
print("empty-range", #move(src, 5, 4, 1, {}), #move(src, 1, 0, 1, {}))

-- M1: metamethods on the source (__index on holes) and the destination (__newindex, __len)
local log = {}
local s_idx = setmetatable({ 1, nil, 3 }, { __index = function(_, k) log[#log + 1] = "i" .. k; return k * 100 end })
local dd = move(s_idx, 1, 3, 1, {})
print("src-index", show(dd, 4), table.concat(log, " "))
log = {}
local nd = setmetatable({}, { __newindex = function(t, k, v) log[#log + 1] = "n" .. k; rawset(t, k, v * 2) end })
move(src, 1, 4, 1, nd)
print("dst-newindex", show(nd, 5), table.concat(log, " "))
local ld = setmetatable({}, { __len = function() return 42 end })
move(src, 1, 4, 1, ld)
print("dst-len", show(ld, 5), #ld, rawlen(ld))
local ld2 = setmetatable({}, { __index = function(_, k) return "dflt" .. k end })
move(src, 1, 3, 1, ld2)
print("dst-index", show(ld2, 5))

-- M1: argument errors (negative / overflowing ranges, wrap-around) leave the destination empty
local function perr(f, ...)
  local ok, e = pcall(f, ...)
  return ok and "ok" or tostring(e):gsub("^.-:%d+: ", "")
end
local e1 = {}
print("err1", perr(move, src, -1, math.maxinteger, 1, e1), next(e1))
print("err2", perr(move, src, 1, 3, math.maxinteger, e1), next(e1))
print("err3", perr(move, src, math.mininteger, -1, 1, e1), next(e1))
print("err4", perr(move, src, 1, 2, 1, "x"))
print("neg", show(move({ [-2] = "a", [-1] = "b", [0] = "c", 1 }, -2, 1, 1, {}), 5))
print("neg-dst", keys(move({ 1, 2, 3 }, 1, 3, -1, {})))

-- R1: remove(t) without a metatable
local t = {}
print("pop-empty", remove(t), #t, next(t))
t = { [0] = "zero" }
print("pop-zero", remove(t), #t, next(t))
t = { 1, 2, 3 }
print("pop", remove(t), remove(t), #t, remove(t), remove(t), #t)
t = { 1, nil, 3 }
print("pop-holes", #t, remove(t), #t, remove(t), #t)
t = { 1, 2, 3, x = "h" }
t[#t + 1] = 4
t[#t + 1] = 5
print("pop-append", remove(t), remove(t), #t, t.x)
t = {}
for i = 1, 40 do t[i] = i end
local sum = 0
while #t > 0 do sum = sum + remove(t) end
print("pop-shrink", sum, #t, next(t))
t = {}
t[1] = 1; t[2] = 2; t[3] = 3; t[100] = 100
print("pop-sparse", #t, remove(t), #t)
t = { 1, 2, 3 }
print("pop-pos", remove(t, 1), remove(t, #t), show(t, 3), remove(t, nil))
t = { 1, 2, 3 }
print("pop-pos-oob", perr(remove, t, 7), perr(remove, t, -1), remove(t, 4), remove(t, #t + 1))
print("pop-notable", perr(remove, "s"), perr(remove, nil), perr(remove))
-- R1 stays off for tables with metatables: __index, __newindex, __len are honoured
log = {}
local mi = setmetatable({ 1, 2 }, { __index = function(_, k) log[#log + 1] = "i" .. k; return "I" .. k end })
print("pop-index", remove(mi), remove(mi), remove(mi), #mi, table.concat(log, " "))
log = {}
local mn = setmetatable({ 1, 2, 3 }, { __newindex = function(t2, k, v) log[#log + 1] = "n" .. k .. tostring(v); rawset(t2, k, v) end })
print("pop-newindex", remove(mn), remove(mn), #mn, table.concat(log, " "))
log = {}
local ml = setmetatable({ 10, 20, 30, 40 }, { __len = function() log[#log + 1] = "l"; return 2 end })
print("pop-len", remove(ml), rawlen(ml), rawget(ml, 2), rawget(ml, 3), table.concat(log, " "))
local ml0 = setmetatable({ [0] = "z" }, { __len = function() return 0 end })
print("pop-len0", remove(ml0), rawget(ml0, 0))
local mb = setmetatable({ 1, 2 }, { __len = function() return "x" end })
print("pop-badlen", perr(remove, mb))

-- the move + pop loop of bench/lib/tremove_pop (hot: traces)
local base = {}
for j = 1, 50 do base[j] = j end
local acc = 0
for o = 1, 300 do
  local tt = move(base, 1, 50, 1, {})
  tt[50] = o
  local v = 0
  for j = 1, 50 do v = v + remove(tt) end
  acc = acc + v + #tt
  if o % 97 == 0 then
    tt[#tt + 1] = "a"
    insert(tt, "b")
    acc = acc + #tt
    setmetatable(tt, { __len = function() return 1 end })
    acc = acc + #tostring(remove(tt))
  end
end
print("loop", acc)
