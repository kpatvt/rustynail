-- T5: GETTABLE / SETTABLE with object keys (tables, functions, threads, userdata) in hot loops: present / absent / nil-valued keys, stores,
-- insertions with rehash, __index / __newindex added mid-run, weak-keyed tables, nil / NaN keys. Same programs as the tr_core T5 programs
-- (crates/rn-jit/tests/tr_core/t5_progs.rs), run here against the reference interpreter by vm_corpus.

-- t5_get_present
do
  print('t5_get_present')
  local function __body(...)
  -- reads of present keys of several object kinds, values of several types, many keys (collision chains), a key kind that changes mid-run
  local function mkkeys(n)
    local ks = {}
    for i = 1, n do ks[i] = {} end
    return ks
  end
  local function sum(set, keys, n)
    local s = 0
    for r = 1, n do
      for i = 1, #keys do
        local v = set[keys[i]]
        if v then s = s + v end
      end
    end
    return s
  end
  local out = {}
  for _, n in ipairs { 1, 2, 3, 5, 8, 13, 40, 200 } do
    local keys = mkkeys(n)
    local set = {}
    for i = 1, n do set[keys[i]] = i end
    for r = 1, 12 do sum(set, keys, 3) end
    out[#out + 1] = sum(set, keys, 5)
  end
  -- float values, then a string value
  local keys = mkkeys(20)
  local set = {}
  for i = 1, 20 do set[keys[i]] = i + 0.5 end
  for r = 1, 30 do sum(set, keys, 3) end
  out[#out + 1] = sum(set, keys, 4)
  set[keys[7]] = 100
  out[#out + 1] = sum(set, keys, 4)
  set[keys[9]] = "11"
  out[#out + 1] = tostring(pcall(sum, set, keys, 4))
  -- functions, coroutines, userdata as keys
  local fks = {}
  for i = 1, 16 do fks[i] = function() return i end end
  local fset = {}
  for i = 1, 16 do fset[fks[i]] = i * 2 end
  for r = 1, 40 do sum(fset, fks, 2) end
  out[#out + 1] = sum(fset, fks, 6)
  local cks = {}
  for i = 1, 8 do cks[i] = coroutine.create(function() end) end
  local cset = {}
  for i = 1, 8 do cset[cks[i]] = i * 3 end
  for r = 1, 40 do sum(cset, cks, 2) end
  out[#out + 1] = sum(cset, cks, 6)
  local uks = { io.stdout, io.stderr, io.stdin }
  local uset = {}
  for i = 1, 3 do uset[uks[i]] = i * 5 end
  for r = 1, 40 do sum(uset, uks, 2) end
  out[#out + 1] = sum(uset, uks, 6)
  -- a mixed key list: tables, functions, threads in one loop (the key tag guard / the walk must not match another kind)
  local mix, mset = {}, {}
  for i = 1, 12 do
    mix[#mix + 1] = {}
    mix[#mix + 1] = function() end
    mix[#mix + 1] = coroutine.create(function() end)
  end
  for i = 1, #mix do if i % 2 == 1 then mset[mix[i]] = i end end
  for r = 1, 40 do sum(mset, mix, 2) end
  out[#out + 1] = sum(mset, mix, 5)
  print(table.concat(out, " "))
  
  end
  __body()
end

-- t5_get_absent
do
  print('t5_get_absent')
  local function __body(...)
  -- absent keys read nil (the walk exits when the key appears); the keyed table gets the key between iterations; keys that are not objects
  local function count(set, keys)
    local c = 0
    for i = 1, #keys do
      if set[keys[i]] == nil then c = c + 1 end
    end
    return c
  end
  local keys = {}
  for i = 1, 30 do keys[i] = {} end
  local set = {}
  for i = 1, 30, 3 do set[keys[i]] = true end
  local out = {}
  for r = 1, 40 do count(set, keys) end
  out[#out + 1] = count(set, keys)
  for i = 1, 30 do
    set[keys[i]] = true
    out[#out + 1] = count(set, keys)
  end
  for i = 1, 30 do
    set[keys[i]] = nil
    out[#out + 1] = count(set, keys)
  end
  -- an empty table, a table with a metatable without __index, then with an __index table and function
  local e = {}
  for r = 1, 40 do count(e, keys) end
  out[#out + 1] = count(e, keys)
  local mt = {}
  setmetatable(e, mt)
  out[#out + 1] = count(e, keys)
  mt.__index = { }
  out[#out + 1] = count(e, keys)
  mt.__index = function(t, k) return 1 end
  out[#out + 1] = count(e, keys)
  mt.__index = nil
  out[#out + 1] = count(e, keys)
  mt.__index = keys
  out[#out + 1] = count(e, keys)
  setmetatable(e, nil)
  out[#out + 1] = count(e, keys)
  -- other key types in the same loop: nil, NaN, floats, booleans, strings, light kinds
  local ot = { [true] = 1, [1.5] = 2, ["s"] = 3 }
  local mixed = { {}, 1.5, true, "s", 7, false, print, io.stdout }
  ot[mixed[1]] = 9
  ot[print] = 10
  local function probe(t, ks)
    local s = 0
    for i = 1, #ks do
      local v = t[ks[i]]
      if v then s = s + v end
    end
    return s
  end
  for r = 1, 40 do probe(ot, mixed) end
  out[#out + 1] = probe(ot, mixed)
  local function nilnan(t, n)
    local c = 0
    for i = 1, n do
      if t[nil] == nil then c = c + 1 end
      if t[0 / 0] == nil then c = c + 1 end
    end
    return c
  end
  out[#out + 1] = nilnan(ot, 30)
  print(table.concat(out, " "))
  
  end
  __body()
end

-- t5_nilvalue_meta
do
  print('t5_nilvalue_meta')
  local function __body(...)
  -- a present key whose value became nil: the read needs the __index guard; the metatable gets an __index later
  local function rd(set, keys)
    local c = 0
    for i = 1, #keys do
      local v = set[keys[i]]
      if v == nil then c = c + 1 elseif v == "mm" then c = c + 100 end
    end
    return c
  end
  local keys = {}
  for i = 1, 16 do keys[i] = {} end
  local set = {}
  for i = 1, 16 do set[keys[i]] = i end
  for i = 1, 16, 2 do set[keys[i]] = nil end
  local out = {}
  for r = 1, 40 do rd(set, keys) end
  out[#out + 1] = rd(set, keys)
  local mt = {}
  setmetatable(set, mt)
  out[#out + 1] = rd(set, keys)
  mt.__index = function(t, k) return "mm" end
  out[#out + 1] = rd(set, keys)
  mt.__index = nil
  out[#out + 1] = rd(set, keys)
  mt.__index = { }
  out[#out + 1] = rd(set, keys)
  -- the same table gets __index while the loop is hot
  local s2 = {}
  for i = 1, 16 do s2[keys[i]] = i end
  for i = 1, 16, 2 do s2[keys[i]] = nil end
  local function churn(n)
    local acc = 0
    for r = 1, n do
      if r == n // 2 then setmetatable(s2, { __index = function() return "mm" end }) end
      acc = acc + rd(s2, keys)
    end
    return acc
  end
  out[#out + 1] = churn(80)
  print(table.concat(out, " "))
  
  end
  __body()
end

-- t5_set_present
do
  print('t5_set_present')
  local function __body(...)
  -- stores over present keys: values of changing types, nil, a table with a metatable (__newindex applies only to absent keys), barriers of collectable values
  local function inc(set, keys, n)
    for r = 1, n do
      for i = 1, #keys do
        local k = keys[i]
        set[k] = (set[k] or 0) + 1
      end
    end
  end
  local keys = {}
  for i = 1, 25 do keys[i] = {} end
  local set = {}
  for i = 1, 25 do set[keys[i]] = i end
  local out = {}
  for r = 1, 20 do inc(set, keys, 2) end
  local s = 0
  for i = 1, 25 do s = s + set[keys[i]] end
  out[#out + 1] = s
  -- collectable values stored over present keys (the table is old by now: the barrier)
  local function put(set, keys, v)
    for i = 1, #keys do set[keys[i]] = v() end
  end
  for r = 1, 30 do put(set, keys, function() return {} end) end
  collectgarbage()
  put(set, keys, function() return { 1 } end)
  collectgarbage("step")
  local t = 0
  for i = 1, 25 do t = t + #set[keys[i]] end
  out[#out + 1] = t
  -- a metatable with __newindex: present keys are raw stores, nil-valued ones go through __newindex
  local log = {}
  local mt = { __newindex = function(t, k, v) log[#log + 1] = v; rawset(t, k, v) end }
  local s3 = setmetatable({}, mt)
  for i = 1, 10 do rawset(s3, keys[i], i) end
  local function wr(s, ks, v)
    for i = 1, #ks do s[ks[i]] = v end
  end
  for r = 1, 40 do wr(s3, keys, r) end
  out[#out + 1] = #log
  for i = 1, 5 do s3[keys[i]] = nil end
  wr(s3, keys, 1000)
  out[#out + 1] = #log
  -- storing nil, then storing again
  local s4 = {}
  for i = 1, 25 do s4[keys[i]] = i end
  local function clear(s, ks)
    for i = 1, #ks do s[ks[i]] = nil end
  end
  for r = 1, 30 do
    clear(s4, keys)
    wr(s4, keys, r)
  end
  local c = 0
  for i = 1, 25 do c = c + s4[keys[i]] end
  out[#out + 1] = c
  print(table.concat(out, " "))
  
  end
  __body()
end

-- t5_insert
do
  print('t5_insert')
  local function __body(...)
  -- insertions of object keys (set[node] = true, number[node] = n): new keys in a loop, a rehash inside the trace, the table keyed by closures / threads
  local function fill(set, keys, v)
    for i = 1, #keys do set[keys[i]] = v end
  end
  local function mk(n)
    local ks = {}
    for i = 1, n do ks[i] = {} end
    return ks
  end
  local out = {}
  for _, n in ipairs { 1, 2, 3, 4, 5, 8, 9, 17, 33, 100, 300 } do
    local keys = mk(n)
    for r = 1, 12 do fill({}, keys, r) end
    local set = {}
    fill(set, keys, 7)
    local c = 0
    for k, v in pairs(set) do c = c + v end
    out[#out + 1] = c
  end
  -- the same table keeps growing across calls (the node vector changes size under the trace)
  local keys = mk(500)
  local grow = {}
  local function addrange(set, a, b)
    for i = a, b do set[keys[i]] = i end
  end
  for i = 1, 500, 10 do addrange(grow, i, i + 9) end
  local c = 0
  for i = 1, 500 do c = c + (grow[keys[i]] or -1) end
  out[#out + 1] = c
  -- keys that are fresh objects created in the loop and held only by the table (the key barrier / GC), then collected through a weak table
  local function fresh(set, n)
    for i = 1, n do set[{}] = i end
  end
  local w = setmetatable({}, { __mode = "k" })
  for r = 1, 30 do fresh(w, 20) end
  collectgarbage()
  collectgarbage()
  local n = 0
  for k in pairs(w) do n = n + 1 end
  out[#out + 1] = n
  local strong = {}
  for r = 1, 30 do fresh(strong, 20) end
  collectgarbage()
  n = 0
  for k, v in pairs(strong) do n = n + v end
  out[#out + 1] = n
  -- closures and coroutines as keys
  local function fills(set, n)
    for i = 1, n do
      local f = function() return i end
      set[f] = f
      set[coroutine.create(f)] = i
    end
  end
  local fs = {}
  for r = 1, 20 do fills(fs, 15) end
  n = 0
  for k, v in pairs(fs) do n = n + 1 end
  out[#out + 1] = n
  -- __newindex on the table (an insertion goes through it)
  local log = 0
  local mt = setmetatable({}, { __newindex = function(t, k, v) log = log + 1; rawset(t, k, v) end })
  for r = 1, 30 do fill(mt, mk(5), r) end
  out[#out + 1] = log
  local mt2 = {}
  local tgt = {}
  setmetatable(mt2, { __newindex = tgt })
  for r = 1, 30 do fill(mt2, mk(5), r) end
  n = 0
  for k in pairs(tgt) do n = n + 1 end
  out[#out + 1] = n .. "/" .. tostring(next(mt2) == nil)
  -- an insertion of nil is a no-op; a nil key is an error
  local function setnil(set, ks)
    for i = 1, #ks do set[ks[i]] = nil end
  end
  local ks = mk(10)
  for r = 1, 30 do setnil({}, ks) end
  local function badkey(set, n)
    for i = 1, n do set[nil] = i end
  end
  out[#out + 1] = tostring(select(2, pcall(badkey, {}, 3)))
  local function nankey(set, n)
    for i = 1, n do set[0 / 0] = i end
  end
  out[#out + 1] = tostring(select(2, pcall(nankey, {}, 3)))
  print(table.concat(out, " "))
  
  end
  __body()
end

-- t5_havlak_shape
do
  print('t5_havlak_shape')
  local function __body(...)
  -- the havlak shape: a DFS numbering through `number[node] = n` / `number[node] == nil`, and a pool set `inPool[x]`, over many nodes and rounds
  local function node(i) return { id = i, out = {} } end
  local nodes = {}
  for i = 1, 120 do nodes[i] = node(i) end
  for i = 1, 120 do
    local o = nodes[i].out
    o[#o + 1] = nodes[(i * 7) % 120 + 1]
    o[#o + 1] = nodes[(i * 11 + 3) % 120 + 1]
  end
  local function dfs(cur, number, cnt)
    number[cur] = cnt
    local last = cnt
    local outs = cur.out
    for i = 1, #outs do
      local t = outs[i]
      if number[t] == nil then last = dfs(t, number, last + 1) end
    end
    return last
  end
  local function pool(nodes, n)
    local inPool, list = {}, {}
    for i = 1, n do
      local x = nodes[(i * 5) % #nodes + 1]
      if not inPool[x] then
        inPool[x] = true
        list[#list + 1] = x
      end
    end
    return #list
  end
  local out = {}
  for r = 1, 30 do
    local number = {}
    out[1] = dfs(nodes[1], number, 0)
    out[2] = pool(nodes, 300)
  end
  local number = {}
  out[3] = dfs(nodes[1], number, 0)
  local s = 0
  for i = 1, 120 do s = s + (number[nodes[i]] or -1) end
  out[4] = s
  print(table.concat(out, " "))
  
  end
  __body()
end

-- t5_gc_small
do
  print('t5_gc_small')
  local function __body(...)
  -- object keys under GC stress: keys that live only in the table, collectable values, a weak-keyed table, rehash inside the loop
  local function ins(set, n)
    for i = 1, n do set[{}] = { i } end
  end
  local function upd(set, keys, v)
    for i = 1, #keys do set[keys[i]] = { v, i } end
  end
  local function look(set, keys)
    local c = 0
    for i = 1, #keys do
      local e = set[keys[i]]
      if e then c = c + e[1] end
    end
    return c
  end
  local keys = {}
  for i = 1, 12 do keys[i] = {} end
  local set = {}
  local out = {}
  for r = 1, 6 do
    upd(set, keys, r)
    ins(set, 9)
    out[#out + 1] = look(set, keys)
  end
  local w = setmetatable({}, { __mode = "k" })
  for r = 1, 5 do ins(w, 7) end
  collectgarbage()
  local n = 0
  for k in pairs(w) do n = n + 1 end
  out[#out + 1] = n
  local fs = {}
  local function fins(set, n)
    for i = 1, n do
      local f = function() return i end
      set[f] = i
    end
  end
  for r = 1, 4 do fins(fs, 6) end
  n = 0
  for k, v in pairs(fs) do n = n + v end
  out[#out + 1] = n
  print(table.concat(out, " "))
  
  end
  __body()
end

-- t5_many_keys
do
  print('t5_many_keys')
  local function __body(...)
  -- 200 present object keys read in a hot loop (one trace serves every key)
  local keys = {}
  for i = 1, 200 do keys[i] = {} end
  local set = {}
  for i = 1, 200 do set[keys[i]] = i end
  local function sum(set, ks)
    local s = 0
    for i = 1, #ks do
      local v = set[ks[i]]
      s = s + v
    end
    return s
  end
  local out = {}
  for r = 1, 60 do out[1] = sum(set, keys) end
  print(out[1])
  
  end
  __body()
end
