-- N2: positional table.insert / table.remove / table.move (bulk element moves on the array part)
local function dump(t, n)
  local o = {}
  for i = 1, n do local v = t[i]; o[#o + 1] = v == nil and "-" or tostring(v) end
  return table.concat(o, ",")
end
local function try(f, ...)
  local ok, e = pcall(f, ...)
  if ok then return "ok" end
  return (tostring(e):gsub("^[^:]*:%d+: ", ""))
end
local function seq(n) local t = {} for i = 1, n do t[i] = i end return t end

-- insert: front, middle, end, n+1, empty table
local t = seq(5)
table.insert(t, 1, "a"); print(#t, dump(t, 6))
table.insert(t, 4, "b"); print(#t, dump(t, 7))
table.insert(t, 7, "c"); print(#t, dump(t, 8))
table.insert(t, #t + 1, "d"); print(#t, dump(t, 9))
local e = {}
table.insert(e, 1, "x"); print(#e, dump(e, 2))
table.insert(e, 1, "y"); print(#e, dump(e, 3))
print(try(table.insert, e, 0, 1))
print(try(table.insert, e, 4, 1))
print(try(table.insert, e, -1, 1))
print(try(table.insert, e, 1))
print(try(table.insert, e, 1, 2, 3))
print(try(table.insert, e))
print(try(table.insert, e, "x", 1))
print(try(table.insert, e, 1.5, 1))
print(try(table.insert, 5, 1, 1))

-- remove: front, middle, last, n+1, empty
t = seq(8)
print(table.remove(t, 1), #t, dump(t, 8))
print(table.remove(t, 3), #t, dump(t, 8))
print(table.remove(t), #t, dump(t, 8))
print(table.remove(t, #t), #t, dump(t, 8))
print(table.remove(t, #t + 1), #t, dump(t, 8))
e = {}
print(table.remove(e), #e)
print(table.remove(e, 0), #e)
print(table.remove(e, 1), #e)
print(try(table.remove, e, 2))
print(try(table.remove, t, 7))
print(try(table.remove, t, -1))
print(try(table.remove, t, 0))
local z = {n = 1}
print(table.remove(z, 0), #z)

-- growth while inserting at the front (the array part is full each time it doubles)
local g = {}
local sum = 0
for i = 1, 300 do table.insert(g, 1, i); sum = sum + g[#g] end
print(#g, sum, g[1], g[150], g[300])
for i = 1, 299 do sum = sum + table.remove(g, 1) * 2 end
print(#g, sum, g[1])
-- queue idiom with strings and tables (GC traffic)
local q = {}
for i = 1, 100 do table.insert(q, 1, {i, tostring(i)}) end
collectgarbage()
local acc = 0
for i = 1, 100 do local x = table.remove(q, 1); acc = acc + x[1] + #x[2] end
print(#q, acc)
for i = 1, 50 do table.insert(q, 1, "s" .. i); table.insert(q, math.floor(#q / 2) + 1, i) end
collectgarbage()
print(#q, dump(q, 12))

-- hash part (keys assigned from the top: no array part)
local h = {}
for i = 6, 1, -1 do h[i] = i * 10 end
table.insert(h, 2, "n"); print(#h, dump(h, 8))
print(table.remove(h, 3), #h, dump(h, 8))
-- array part plus hash part tail
local m = {1, 2, 3}
m[5] = 5; m[4] = 4
table.insert(m, 1, 0); print(#m, dump(m, 7))
print(table.remove(m, 2), #m, dump(m, 7))

-- nil values inside the moved range (explicit sizes through table.move)
local hl = {1, nil, 3, nil, 5, n = 5}
table.move(hl, 1, 5, 2); print(dump(hl, 6))
table.move(hl, 2, 6, 1); print(dump(hl, 6))
local hl2 = {nil, nil, 3}
print(#hl2, table.remove(hl2, 1), dump(hl2, 3))

-- move: overlapping forward and backward, different tables, outside the array part
t = seq(10)
table.move(t, 1, 5, 3); print(dump(t, 10))
t = seq(10)
table.move(t, 3, 8, 1); print(dump(t, 10))
t = seq(10)
table.move(t, 1, 10, 1); print(dump(t, 10))
t = seq(10)
table.move(t, 1, 3, 9); print(dump(t, 12))
t = seq(10)
table.move(t, 5, 4, 1); print(dump(t, 10))
t = seq(10)
table.move(t, 0, 2, 5); print(dump(t, 10), t[0])
t = seq(10)
table.move(t, -1, 1, 5); print(dump(t, 10))
local d = {}
print(dump(table.move(seq(5), 2, 4, 1, d), 5), dump(d, 5))
d = {"a", "b", "c", "d", "e", "f"}
print(dump(table.move(d, 1, 3, 4), 6))
print(try(table.move, seq(3), 1, math.maxinteger, 2))
print(try(table.move, seq(3), -1, math.maxinteger, 2))
print(try(table.move, seq(3), 1, 2, math.maxinteger))
print(try(table.move, seq(3), 1, 2))
print(try(table.move, 1, 1, 2, 3))
-- move 1000 elements back and forth
local big = seq(1000)
for r = 1, 30 do table.move(big, 1, 999, 2); big[1] = r end
print(big[1], big[2], big[500], big[1000])
for r = 1, 30 do table.move(big, 2, 1000, 1) end
print(big[1], big[500], big[1000])

-- metatables: no relevant metamethod (class-like), then __index / __newindex / __len
local cls = {}
cls.__index = nil
local o = setmetatable(seq(6), {__name = "x"})
table.insert(o, 2, "i"); print(#o, dump(o, 7))
print(table.remove(o, 1), #o, dump(o, 7))
table.move(o, 1, 3, 2); print(dump(o, 7))

local log = {}
local function logger(name) return function(t, k, v) log[#log + 1] = name .. k; return rawget(t, k) end end
local px = setmetatable({1, 2, nil, 4}, {__index = function(t, k) log[#log + 1] = "g" .. k; return "d" .. k end})
-- holes + __index: the moved range reads the default
print(try(table.insert, px, 1, "v"), dump(px, 6), table.concat(log, " "))
log = {}
local nw = setmetatable({}, {__newindex = function(t, k, v) log[#log + 1] = "s" .. k; rawset(t, k, v) end})
for i = 1, 4 do nw[i] = i end
table.insert(nw, 1, 0); print(dump(nw, 6), table.concat(log, " "))
log = {}
print(table.remove(nw, 1), dump(nw, 6), table.concat(log, " "))
log = {}
local ln = setmetatable({1, 2, 3}, {__len = function() return 5 end})
print(try(table.insert, ln, 1, "q"), rawlen(ln), dump(ln, 6))
print(table.remove(ln, 2), rawlen(ln), dump(ln, 6))
-- an array part that is not full under __newindex: writes into the empty slot go to the metamethod
local sp = setmetatable({1, 2, 3}, {__newindex = function(t, k, v) log[#log + 1] = "n" .. k; rawset(t, k, v) end})
table.insert(sp, 1, 0); print(dump(sp, 5), table.concat(log, " "))
-- proxy: all accesses go through the metamethods
local store = {10, 20, 30}
local proxy = setmetatable({}, {
  __index = function(_, k) return store[k] end,
  __newindex = function(_, k, v) store[k] = v end,
  __len = function() return #store end,
})
table.insert(proxy, 1, 5); print(dump(store, 5))
print(table.remove(proxy, 2), dump(store, 5))
table.move(proxy, 1, 3, 2); print(dump(store, 5))
-- the metatable gains __index/__newindex after the first (metamethod-free) operations
local late = {__name = "late"}
local lt = setmetatable(seq(4), late)
table.insert(lt, 1, 0); print(dump(lt, 6))
log = {}
late.__newindex = function(t, k, v) log[#log + 1] = "n" .. k; rawset(t, k, v) end
lt[7] = nil
table.insert(lt, 1, -1); print(dump(lt, 7), table.concat(log, " "))
log = {}
late.__index = function(t, k) log[#log + 1] = "i" .. k; return 0 end
lt[2] = nil
print(try(table.insert, lt, 1, 9), dump(lt, 7), table.concat(log, " "))
-- weak table (has a metatable)
local wk = setmetatable(seq(5), {__mode = "v"})
table.insert(wk, 1, "w"); print(dump(wk, 6))
-- same-table move with a metatable present
table.move(late, 1, 0, 1)
print(dump(table.move(lt, 1, 3, 2), 5), table.concat(log, " "))
-- table.move between a plain table and a proxy
log = {}
local dst = setmetatable({}, {__newindex = function(t, k, v) log[#log + 1] = k; rawset(t, k, v) end})
table.move(seq(3), 1, 3, 1, dst); print(dump(dst, 3), table.concat(log, " "))

-- resize during the move: __newindex that grows the table
local grow = setmetatable(seq(4), {__newindex = function(t, k, v)
  for i = 5, 40 do rawset(t, i, i) end
  rawset(t, k, v)
end})
table.insert(grow, 1, "G"); print(#grow, dump(grow, 8))

-- the length of a table with a hole, then positional operations (border agreement with the loop)
local b = {}
for i = 1, 20 do b[i] = i end
b[10] = nil
local n0 = #b
print(n0 == 20 or n0 == 9)

-- checksum of a long mixed sequence
local ck, w = 0, seq(64)
for i = 1, 4000 do
  local op = i % 5
  local n = #w
  if op == 0 and n > 1 then ck = ck + table.remove(w, (i * 7) % n + 1)
  elseif op == 1 then table.insert(w, (i * 13) % (n + 1) + 1, i)
  elseif op == 2 and n > 3 then table.move(w, 2, n, 1)
  elseif op == 3 then table.insert(w, 1, i); table.remove(w)
  else table.insert(w, i) end
end
print(#w, ck, w[1], w[#w], w[#w // 2])

-- GC: collectable values moved into an old / black table survive the next collections
for _, mode in ipairs{ "incremental", "generational" } do
  collectgarbage(mode)
  local old = {1, 2, 3, 4, 5, 6, 7, 8}
  collectgarbage(); collectgarbage()
  for r = 1, 6 do
    local src = {}
    for i = 1, 8 do src[i] = {r, i, ("v" .. r .. "_" .. i):rep(3)} end
    table.move(src, 1, 8, 1, old)
    src = nil
    collectgarbage("step", 1)
    table.insert(old, 1, {r, 0, "front"}); table.remove(old)
    collectgarbage("step", 1)
    if r % 2 == 0 then collectgarbage() end
    local o = {}
    for i = 1, 8 do local v = old[i]; o[#o + 1] = v[1] .. ":" .. v[2] .. ":" .. #v[3] end
    print(mode, r, table.concat(o, " "))
  end
end
collectgarbage("incremental")
