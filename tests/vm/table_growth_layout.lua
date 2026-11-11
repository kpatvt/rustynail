-- Table growth layout (C22): construction shapes that exercise the small array / small hash growth paths (1 -> 2 -> 4 -> 8 ...
-- slots and nodes, appends at asize + 1 with a dense or holey array, with and without a hash part, integer keys in the hash
-- part, deleted nodes). For each shape: pairs order (keys sorted when there are string keys: their order depends on the hash
-- seed), '#', and the exact number of bytes `collectgarbage('count')` accounts for the table. Outputs are compared with the
-- reference Lua (any collector mode: the collector is stopped while building).
collectgarbage("stop")

local names = {}
-- float keys: a hash that does not depend on the string-hash seed, so the node layout (and with it the exact sizes after
-- deletions) is the same in every run; tests/vm_seed0/table_growth_layout_str.lua is this file with string keys (RUSTYNAIL_SEED=0)
for i = 1, 12 do names[i] = i + 0.5 end

-- (Lua 5.4: the object sizes of the two implementations differ by design -- docs/STATUS.md: reference `Table` 56 bytes + 16 per array slot,
-- ours 48 + 9 -- so the byte columns cannot match the 5.4 reference; they are a constant there and everything else is still compared)
local function bytes() if _VERSION == "Lua 5.4" then return 0 end return math.floor(collectgarbage("count") * 1024 + 0.5) end

local function keys_of(t)
  local parts, hasstr = {}, false
  for k, v in pairs(t) do
    if type(k) == "string" then hasstr = true end
    parts[#parts + 1] = math.type(k) == "float" and ("f" .. tostring(k)) or tostring(k)
  end
  if hasstr then table.sort(parts) end
  return table.concat(parts, ",")
end

-- '#' of a table with integer keys in the hash part depends on a seed-dependent probe: print only its validity
local function border(t, exact)
  local n = #t
  if exact then return tostring(n) end
  return ((n == 0 or t[n] ~= nil) and t[n + 1] == nil) and "ok" or ("BAD" .. n)
end

local keep = {}
local function shape(name, exact, build)
  local b0 = bytes()
  local t = build()
  local b1 = bytes()
  keep[#keep + 1] = t
  print(name, keys_of(t), "#" .. border(t, exact), "bytes", b1 - b0)
end

-- appends
for n = 0, 20 do
  shape("append" .. n, true, function()
    local t = {}
    for i = 1, n do t[#t + 1] = i end
    return t
  end)
end
for n = 0, 20 do
  shape("index" .. n, true, function()
    local t = {}
    for i = 1, n do t[i] = i end
    return t
  end)
end
for n = 1, 12 do
  shape("reverse" .. n, false, function()
    local t = {}
    for i = n, 1, -1 do t[i] = i end
    return t
  end)
end
for n = 1, 10 do
  shape("insert" .. n, true, function()
    local t = {}
    for i = 1, n do table.insert(t, i) end
    return t
  end)
end

-- objects: string fields stored one by one (hash growth 0 -> 1 -> 2 -> 4 -> 8 -> 16)
local C = {}
C.__index = C
for n = 1, 12 do
  shape("object" .. n, false, function()
    local o = setmetatable({}, C)
    for i = 1, n do o[names[i]] = i end
    return o
  end)
end
for n = 1, 12 do
  shape("objdel" .. n, false, function()
    local o = {}
    for i = 1, n do o[names[i]] = i end
    for i = 1, n, 2 do o[names[i]] = nil end
    for i = 1, 3 do o[names[(i * 5) % 12 + 1]] = i end
    return o
  end)
end

-- hash part first, then appends (array growth next to a hash part, with and without deleted nodes)
for nf = 0, 5 do
  for na = 0, 9 do
    shape("hash" .. nf .. "_arr" .. na, false, function()
      local t = {}
      for i = 1, nf do t[names[i]] = i end
      for i = 1, na do t[#t + 1] = i end
      return t
    end)
  end
end
for nf = 1, 4 do
  for na = 1, 6 do
    shape("arr" .. na .. "_hash" .. nf, false, function()
      local t = {}
      for i = 1, na do t[i] = i end
      for i = 1, nf do t[names[i]] = i end
      return t
    end)
  end
end
for nf = 1, 4 do
  shape("delhash" .. nf, false, function()
    local t = {}
    for i = 1, nf do t[names[i]] = i end
    for i = 1, nf do t[names[i]] = nil end
    for i = 1, 6 do t[#t + 1] = i end
    return t
  end)
end

-- holes: array with nil slots, then an append at asize + 1 (the dense shortcut must not apply)
for n = 1, 9 do
  for hole = 1, n do
    shape("hole" .. n .. "_" .. hole, false, function()
      local t = {}
      for i = 1, n do t[i] = i end
      t[hole] = nil
      t[n + 1] = 0
      t[n + 2] = 0
      return t
    end)
  end
end
for n = 1, 8 do
  shape("holes_tail" .. n, false, function()
    local t = {1, 2, 3, 4, 5, 6, 7, 8, 9}
    for i = n, 8 do t[i] = nil end
    t[10] = 1
    t[11] = 1
    return t
  end)
end

-- integer keys in the hash part when the array grows (they move into the array part)
for gap = 0, 4 do
  for n = 1, 6 do
    shape("intfar" .. n .. "_" .. gap, false, function()
      local t = {}
      for i = 1, n do t[i] = i end
      t[n + 2 + gap] = 1
      t[n + 1] = 1
      t[n + 3 + gap] = 1
      t[#t + 1] = 1
      return t
    end)
  end
end
for n = 0, 5 do
  shape("sparse" .. n, false, function()
    local t = {}
    for i = 1, n do t[i * 10] = i end
    t[1] = 1
    t[2] = 1
    t[3] = 1
    return t
  end)
end

-- constructors with sizes, then growth
shape("ctor3_4", true, function() local t = {1, 2, 3}; t[4] = 4; t[5] = 5; return t end)
shape("ctor1_2", true, function() local t = {1}; t[2] = 2; t[3] = 3; t[4] = 4; t[5] = 5; return t end)
shape("ctor2f", false, function() local t = {x = 1, y = 2}; t[1] = 1; t[2] = 2; t[3] = 3; return t end)
shape("ctor2f_obj", false, function() local t = {x = 1, y = 2}; t.z = 3; t.w = 4; t.v = 5; return t end)
shape("ctor_mixed", false, function() local t = {1, 2, x = 1}; t[3] = 3; t.y = 2; t[4] = 4; t[5] = 5; return t end)
shape("ctor0", true, function() local t = {nil, nil, 3}; t[4] = 4; t[5] = 5; return t end)

-- shrinking and removing from the end, then growing again
shape("pop_push", true, function()
  local t = {}
  for i = 1, 6 do t[#t + 1] = i end
  for i = 1, 5 do t[#t] = nil end
  for i = 1, 7 do t[#t + 1] = i end
  return t
end)
shape("move", true, function()
  local t = {}
  for i = 1, 5 do t[#t + 1] = i end
  return table.move(t, 1, 5, 4, {})
end)
shape("pack", true, function() return table.pack(1, 2, 3) end)
shape("unpack_rt", true, function() return {table.unpack({1, 2, 3, 4, 5}, 2, 5)} end)

-- pseudo-random op sequences: layout fingerprint after every op (full lines for the first seeds, a checksum for the rest)
local seed = 42
local function rnd(n)
  seed = (seed * 1103515245 + 12345) % 2147483648
  return seed % n + 1
end
local function fingerprint(t)
  local s = keys_of(t) .. "|" .. border(t, false)
  return s
end
local sum = 0
local function mix(s)
  for i = 1, #s do sum = (sum * 31 + s:byte(i)) % 1000000007 end
end
for r = 1, 400 do
  local t = {}
  local mode = rnd(4)
  local nops = 5 + rnd(30)
  local top = 0
  local lines = {}
  for step = 1, nops do
    local k
    local what = rnd(8)  -- ('top': not '#t', whose value for tables with integer keys in the hash part is seed-dependent)
    if what <= 3 then k = (mode == 1) and (top + 1) or (rnd(10))
    elseif what <= 5 then k = names[rnd(6)]
    elseif what == 6 then k = (mode == 2) and (top + 1) or (rnd(4) * 3)
    else k = rnd(20) end
    if k == top + 1 then top = k end
    local bb = bytes()
    if rnd(5) == 1 then t[k] = nil else t[k] = step end
    local d = bytes() - bb
    local line = fingerprint(t) .. "|" .. d
    if r <= 6 then lines[#lines + 1] = line else mix(line) end
  end
  keep[#keep + 1] = t
  if r <= 6 then print("rand" .. r, table.concat(lines, ";")) end
end
print("randsum", sum)
