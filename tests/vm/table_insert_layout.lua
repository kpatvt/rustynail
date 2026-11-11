-- Table insertion layout: growth patterns, pairs/next order, '#' borders and unpack over deterministic
-- pseudo-random insert/remove sequences with every key kind. Layout-exactness check of the insert path
-- (finish_set / newkey / insertkey / rehash): outputs are compared with the reference Lua.
local seed = 12345
local function rnd(n)
  seed = (seed * 1103515245 + 12345) % 2147483648
  return seed % n + 1
end

-- With string keys the traversal order depends on the string-hash seed (the harness does not pin ours): those
-- tables print their keys sorted, tables without string keys print them in traversal order.
-- (5.5 starts the border search in the hash part at a seed-dependent probe: `lenmode` prints only whether `#t`
-- is a valid border for tables built by random inserts, not its value)
local function border(t, lenmode)
  local n = #t
  if lenmode then
    return (n == 0 or t[n] ~= nil) and t[n + 1] == nil and "ok" or ("BAD" .. n)
  end
  return n
end

local function dump(t, lenmode)
  local parts, n, hasstr = {}, 0, false
  for k, v in pairs(t) do
    n = n + 1
    if type(k) == "string" then hasstr = true end
    local ks = type(k) == "table" and "tbl" or tostring(k)
    parts[#parts + 1] = math.type(k) == "float" and ("f" .. ks) or ks
  end
  if hasstr then table.sort(parts) end
  return table.concat(parts, ",") .. " #" .. border(t, lenmode) .. " n" .. n
end

local longs = {}
for i = 1, 6 do longs[i] = string.rep("L" .. i, 20) end
local tkeys = {}
for i = 1, 4 do tkeys[i] = {} end

local function key(kind)
  if kind == 1 then return rnd(12)                      -- small ints
  elseif kind == 2 then return rnd(200) + 1000           -- sparse ints
  elseif kind == 3 then return rnd(10) + 0.0             -- floats with int values
  elseif kind == 4 then return rnd(10) + 0.5             -- real floats
  elseif kind == 5 then return "s" .. rnd(30)            -- short strings
  elseif kind == 6 then return longs[rnd(6)]             -- long strings
  elseif kind == 7 then return rnd(2) == 1               -- booleans
  elseif kind == 8 then return tkeys[rnd(4)]             -- tables
  elseif kind == 9 then return -rnd(8)                   -- negative ints
  else return rnd(3) * 2^40 end                          -- large ints/floats
end

-- ordered kinds only (no table keys: their order depends on addresses)
local function run(kinds, nops, removeprob)
  local t = {}
  for step = 1, nops do
    local k = key(kinds[rnd(#kinds)])
    if k ~= k then k = 1 end
    if rnd(100) <= removeprob then
      t[k] = nil
    else
      t[k] = step
    end
    if step % 7 == 0 then
      -- no table keys in the ordered dump
      local d = dump(t, true)
      if not (type(k) == "table") then io.write(d, "\n") end
    end
  end
  io.write("final ", dump(t, true), "\n")
  io.write("unpack ", select("#", table.unpack(t, 1, #t)) == #t and "ok" or "BAD", "\n")
end

local kindsets = {
  {1}, {1, 2}, {1, 3}, {1, 4, 5}, {5}, {5, 6}, {2, 9}, {1, 7}, {1, 2, 3, 4, 5, 6, 7, 9, 10},
  {3, 10}, {6, 7}, {1, 9},
}
for si, ks in ipairs(kindsets) do
  for _, rp in ipairs({0, 20, 50}) do
    io.write("-- set ", si, " rm ", rp, "\n")
    run(ks, 70, rp)
  end
end

-- table keys: only the count and lookups are address independent
do
  local t = {}
  for i = 1, 4 do t[tkeys[i]] = i end
  t[1] = 1; t[2] = 2; t.x = 3
  local n = 0
  for k, v in pairs(t) do n = n + 1 end
  io.write("tblkeys ", n, " ", t[tkeys[3]], " #", #t, "\n")
end

-- growth patterns of fresh tables
for n = 0, 20 do
  local t = {}
  for i = 1, n do t[i] = i end
  io.write("seq", n, " ", dump(t), "\n")
  local u = {}
  for i = n, 1, -1 do u[i] = i end
  io.write("rev", n, " ", dump(u), "\n")
  local w = {}
  for i = 1, n do w["f" .. i] = i end
  io.write("str", n, " ", dump(w), "\n")
  local m = {}
  for i = 1, n do m[i] = i; m["g" .. i] = i end
  io.write("mix", n, " ", dump(m), "\n")
  local a = {}
  for i = 1, n do a[#a + 1] = i end
  io.write("app", n, " ", dump(a), "\n")
  local h = {}
  for i = 1, n do h[i * 3] = i end
  io.write("step3 ", n, " ", dump(h), "\n")
end

-- constructors then growth, holes and borders
do
  local t = {1, 2, 3}
  t[5] = 5; io.write(dump(t), "\n")
  t[4] = 4; io.write(dump(t), "\n")
  t[8] = 8; t[7] = 7; t[6] = 6; io.write(dump(t), "\n")
  local u = {n = 1, 10, 20, nil, 40}
  u[#u + 1] = 50; io.write(dump(u), "\n")
  u[7] = 7; u[6] = 6; io.write(dump(u), "\n")
  local v = {x = 1, y = 2}
  v[1] = 1; v.z = 3; v[2] = 2; v.w = 4; v[3] = 3
  io.write(dump(v), "\n")
  for i = 1, 5 do v[i] = nil end
  v.q = 1; v.r = 2; v.s = 3
  io.write(dump(v), "\n")
end

-- table.insert / remove / move / unpack
do
  local t = {}
  for i = 1, 10 do table.insert(t, i) end
  table.insert(t, 1, 0); table.insert(t, 5, 55); table.insert(t, #t + 1, 99)
  io.write(dump(t), "\n")
  io.write(tostring(table.remove(t)), tostring(table.remove(t, 1)), tostring(table.remove(t, 4)), " ", dump(t), "\n")
  local m = table.move(t, 2, 6, 10)
  io.write(dump(m), "\n")
  local c = table.move(t, 1, #t, 1, {})
  io.write(dump(c), "\n")
  local d = table.move({1, 2, 3}, 1, 3, 3)
  io.write(dump(d), "\n")
  io.write(select("#", table.unpack({1, nil, 3}, 1, 3)), " ", select("#", table.unpack({n = 3}, 1, 3)), "\n")
  io.write(select("#", table.unpack(t)), "\n")
  io.write(dump(table.pack(1, nil, 3)), "\n")
end

-- errors: nil and NaN keys
print(pcall(function() local t = {}; t[nil] = 1 end))
print(pcall(function() local t = {}; t[0/0] = 1 end))
print(pcall(function() local t = {}; t[1] = 1; t[nil] = 2 end))
print(pcall(function() local t = {a = 1}; t[0/0] = 2 end))
print(pcall(rawset, {}, nil, 1))
print(pcall(rawset, {}, 0/0, 1))
do
  local t = {}
  t[1.0] = "a"; t[2^53] = "b"; t[-0.0] = "z"
  io.write(dump(t), " ", tostring(t[1]), " ", tostring(t[0]), "\n")
end

-- dynamic string keys into the same table, with removals and reinsertion
do
  local t = {}
  local keys = {}
  for i = 1, 40 do keys[i] = "k" .. i end
  for r = 1, 3 do
    for i = 1, 40, r do t[keys[i]] = i end
    for i = 2, 40, 3 do t[keys[i]] = nil end
    io.write(dump(t), "\n")
  end
  local nn = 0
  local k, v = next(t)
  while k do nn = nn + 1; k, v = next(t, k) end
  io.write("next ", nn, "\n")
end

-- constructor idiom: fresh table, 1..5 fields then class-style use
do
  local n = 0
  for i = 1, 50 do
    local o = {}
    o.x = i; o.y = i; if i % 2 == 0 then o.z = i end; if i % 3 == 0 then o[1] = i; o[2] = i end
    if i % 5 == 0 then o.w = i; o[#o + 1] = i end
    local s = dump(o)
    n = n + #s
    if i % 10 == 0 then io.write(s, "\n") end
  end
  io.write("total ", n, "\n")
end
