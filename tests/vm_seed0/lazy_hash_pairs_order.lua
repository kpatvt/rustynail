-- Lazy short-string hash ("hash-D"): the order of `pairs` depends on the PUC hash of the keys, which the lazy scheme computes
-- only when a string first becomes a table key. Run-time created keys through every kind of producer and every kind of
-- insert path; pairs order printed UNSORTED, so the output differs from the reference unless every key got its PUC hash
-- (compare with the reference only with RUSTYNAIL_SEED=0: not part of the tests/vm corpus; checked by crates/rn-jit/tests/tr_hashd.rs).
collectgarbage("stop")

local function order(t)
  local parts = {}
  for k, v in pairs(t) do parts[#parts + 1] = tostring(k) .. "=" .. tostring(v) end
  return table.concat(parts, " ")
end

local function mk(i) -- one string per producer
  local n = i % 12
  if n == 0 then return "k" .. i
  elseif n == 1 then return string.format("fmt_%d_%s", i, "x")
  elseif n == 2 then return ("ab"):rep(1 + i % 5) .. i
  elseif n == 3 then return (("hello world %d"):format(i)):sub(3, -2)
  elseif n == 4 then return string.char(97 + i % 26, 98 + i % 20, 65 + i % 26)
  elseif n == 5 then return tostring(i * 7)
  elseif n == 6 then return table.concat({ "a", i, "b" }, "-")
  elseif n == 7 then return (("Key" .. i):upper()):lower()
  elseif n == 8 then return ("x" .. i):reverse()
  elseif n == 9 then return (("zz" .. i):gsub("z", "y"))
  elseif n == 10 then return utf8.char(100 + i % 50) .. i
  else return select(2, ("pre" .. i):match("(pre)(%d+)")) .. "!" end
end

-- 1. plain assignment, growing from empty (rehash at 1, 2, 4, 8 ... 512)
for _, size in ipairs { 1, 2, 3, 5, 8, 9, 17, 33, 100, 513 } do
  local t = {}
  for i = 1, size do t[mk(i)] = i end
  print("assign", size, order(t))
end

-- 2. lookups of absent run-time keys BEFORE they are inserted (must not leave a raw hash behind)
do
  local t = { alpha = 1, beta = 2, gamma = 3 }
  local keys = {}
  for i = 1, 40 do keys[i] = mk(i) end
  local miss = 0
  for i = 1, 40 do if t[keys[i]] == nil then miss = miss + 1 end end
  for i = 1, 40 do if rawget(t, keys[i]) == nil then miss = miss + 1 end end
  for i = 1, 40 do t[keys[i]] = i end
  print("lookup-first", miss, order(t))
  -- and the keys are found afterwards through freshly built equal strings
  local hit = 0
  for i = 1, 40 do if t[mk(i)] == i then hit = hit + 1 end end
  print("found", hit)
end

-- 3. rawset, constructor with computed keys, copying, __newindex + rawset
do
  local t = {}
  for i = 1, 30 do rawset(t, mk(i), i) end
  print("rawset", order(t))
  local u = { [mk(1)] = 1, [mk(2)] = 2, [mk(3)] = 3, [mk(4)] = 4, [mk(5)] = 5, [mk(6)] = 6, [mk(7)] = 7 }
  print("ctor", order(u))
  local v = setmetatable({}, { __newindex = function(tb, k, x) rawset(tb, k, x * 2) end })
  for i = 1, 20 do v[mk(i)] = i end
  print("newindex", order(v))
  local w = {}
  for k, x in pairs(t) do w[k] = x end
  print("copy", order(w))
end

-- 4. deletions and reinsertions (dead keys keep their node; reinsertion goes through the same main position)
do
  local t = {}
  for i = 1, 24 do t[mk(i)] = i end
  for i = 1, 24, 2 do t[mk(i)] = nil end
  for i = 1, 24, 3 do t[mk(i)] = -i end
  for i = 25, 40 do t[mk(i)] = i end
  print("delete", order(t))
end

-- 5. globals (_ENV table) and _G through run-time names
do
  for i = 1, 20 do _G["glob" .. mk(i)] = i end
  local names = {}
  for k in pairs(_G) do if k:find("^glob") then names[#names + 1] = k end end
  print("globals", table.concat(names, " "))
  for i = 1, 20 do _G["glob" .. mk(i)] = nil end
end

-- 6. the same strings used in two tables in opposite insertion order
do
  local a, b = {}, {}
  for i = 1, 15 do local k = mk(i); a[k] = i end
  for i = 15, 1, -1 do local k = mk(i); b[k] = i end
  print("shared", order(a), "|", order(b))
end

-- 7. package.preload with run-time names, string.pack keys
do
  for i = 1, 10 do package.preload[mk(i)] = function() return i end end
  local ks = {}
  for k in pairs(package.preload) do ks[#ks + 1] = k end
  print("preload", table.concat(ks, " "))
  for i = 1, 10 do package.preload[mk(i)] = nil end
  local t = {}
  for i = 1, 12 do t[string.pack("<I2", i * 257)] = i end
  local o = {}
  for k, v in pairs(t) do o[#o + 1] = v end
  print("pack", table.concat(o, " "))
end

-- 8. mixed keys: floats, integers in the hash part, booleans, strings (no table keys: their order depends on addresses) (order of the string keys only)
do
  local t = {}
  for i = 1, 20 do
    t[mk(i)] = i
    t[i * 1.5] = i
    t[-i] = i
    t[i * 1000003] = i
  end
  t[true] = 1; t[false] = 2
  local n, s = 0, 0
  local ks = {}
  for k in pairs(t) do
    if type(k) == "string" then n = n + 1; s = s + #k; ks[#ks + 1] = k end
  end
  print("mixed", n, s, table.concat(ks, " "))
end

-- 9. the same strings re-created after a collection (new objects, same bytes)
collectgarbage("restart")
do
  local t = {}
  for i = 1, 50 do t[mk(i)] = i end
  collectgarbage(); collectgarbage()
  local u = {}
  for i = 1, 50 do u[mk(i)] = i end
  print("gc", order(t) == order(u), order(u))
end

-- 10. weak-keyed table (strings are values, not collectable keys)
do
  local t = setmetatable({}, { __mode = "k" })
  for i = 1, 20 do t[mk(i)] = i end
  print("weak", order(t))
end
