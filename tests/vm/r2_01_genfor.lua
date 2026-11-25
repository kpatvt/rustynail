-- docs/JIT_TRACE.md section 54 (R2, `t1g`): the tier-1 generic-`for` paths: `CLOSE` without its helper when nothing is open, `TFORCALL` of `next` through
-- the lean helper, the four results of an iterator factory moved inline. Same output as the reference in every mode (`pairs` order follows the seed).

local function keys(t)
  local ks = {}
  for k, v in pairs(t) do ks[#ks + 1] = tostring(k) .. "=" .. tostring(v) end
  return table.concat(ks, ",")
end

-- (1) pairs / next over hash, array and mixed tables, empty tables, nested loops, early breaks
local T = { north = 1, east = 2, down = 3, 10, 20, 30, [7] = "seven", [2.5] = "f", [true] = "t" }
for r = 1, 4 do
  local n, s = 0, 0
  for k, v in pairs(T) do n = n + 1 if type(v) == "number" then s = s + v end end
  print("pairs", r, n, s, keys(T))
  local m = 0
  for k in next, T do m = m + 1 end
  for k, v in next, T, nil do m = m + 1 end
  print("next", r, m)
  for k in pairs({}) do print("never", k) end
  local c = 0
  for k1 in pairs(T) do for k2 in pairs(T) do c = c + 1 if c % 17 == 0 then break end end end
  print("nested", r, c)
end

-- (2) assignments to existing fields and clearing fields during the traversal (allowed), then the traversal of the result
local U = {}
for i = 1, 20 do U["k" .. i] = i end
for r = 1, 3 do
  for k, v in pairs(U) do
    if v % 3 == 0 then U[k] = nil else U[k] = v + 100 end
  end
  print("modified", r, keys(U))
end

-- (3) an invalid key to `next` raises the reference's error (the lean helper declines, the general path raises)
for r = 1, 3 do
  local ok, err = pcall(function()
    local t = { a = 1, b = 2 }
    for k, v in next, t, "nosuchkey" do print("unreached", k) end
  end)
  print("badkey", r, ok, err)
  print("badnext", r, pcall(next, { x = 1 }, "zz"))
end

-- (4) ipairs over tables with holes and a hash part (the end of the array is the general helper's: it must not be taken for `next`)
local A = { 1, 2, 3, x = 5, y = 6, [5] = 50 }
for r = 1, 3 do
  local out = {}
  for i, v in ipairs(A) do out[#out + 1] = i .. ":" .. tostring(v) end
  print("ipairs", r, table.concat(out, " "))
  local cnt = 0
  for i, v in ipairs({}) do cnt = cnt + 1 end
  for i, v in ipairs({ nil, 2 }) do cnt = cnt + 1 end
  print("ipairs-empty", r, cnt)
end

-- (5) other iterators: light C functions that are not `next`, C closures (gmatch), Lua closures, __pairs
local function liter(t, i) i = i + 1 local v = t[i] if v then return i, v end end
local P = setmetatable({}, { __pairs = function(t) return function(_, k) if not k then return 1, "one" elseif k < 3 then return k + 1, "more" end end, t, nil end })
for r = 1, 3 do
  local w = {}
  for word in string.gmatch("take the lamp and go", "%a+") do w[#w + 1] = word end
  local l = {}
  for i, v in liter, { "a", "b", "c" }, 0 do l[#l + 1] = v end
  local p = {}
  for k, v in pairs(P) do p[#p + 1] = k .. v end
  print("iters", r, table.concat(w, "|"), table.concat(l), table.concat(p, ","))
  print("select", r, select("#", ipairs(A)), select("#", pairs(A)), (select(2, pairs(A))) == A)
end

-- (6) CLOSE with captured loop variables: every closure keeps its own value
for r = 1, 3 do
  local fs = {}
  for i = 1, 5 do
    local j = i * r
    fs[i] = function() return j end
  end
  for k, v in pairs({ a = 1, b = 2, c = 3 }) do
    local kv = k .. v
    fs[#fs + 1] = function() return kv end
  end
  local got = {}
  for i = 1, 5 do got[i] = fs[i]() end
  local tail = {}
  for i = 6, #fs do tail[#tail + 1] = fs[i]() end
  table.sort(tail)
  print("closures", r, table.concat(got, ","), table.concat(tail, ","))
  -- a closure created below the level of an inner block, the inner block closing nothing of it
  local outer = 0
  local f = function() return outer end
  for i = 1, 3 do do local z = i outer = outer + z end end
  print("outer", r, f())
end

-- (7) to-be-closed variables in blocks and loops: __close runs at every block exit, in order
local log = {}
local function closer(name) return setmetatable({}, { __close = function() log[#log + 1] = name end }) end
for r = 1, 3 do
  log = {}
  for i = 1, 3 do
    do
      local x <close> = closer("x" .. i)
      local y = i
      do local z = y + 1 log[#log + 1] = "z" .. z end
    end
    log[#log + 1] = "after" .. i
  end
  do
    local a <close> = closer("a")
    for k in pairs({ p = 1 }) do local inner = k log[#log + 1] = "in" .. inner end
    log[#log + 1] = "body"
  end
  print("tbc", r, table.concat(log, " "))
end

-- (8) a long traversal (hash part growth, rehash between traversals) and a GC cycle in the middle
for r = 1, 2 do
  local big = {}
  for i = 1, 300 do big["key" .. i] = i end
  local s, n = 0, 0
  for k, v in pairs(big) do
    s = s + v n = n + 1
    if n == 150 then collectgarbage() end
  end
  print("big", r, n, s)
end
