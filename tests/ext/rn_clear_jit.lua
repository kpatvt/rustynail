-- table.clear in hot loops: compiled code (tier 1 / traces) caches array sizes, node positions and shapes of tables;
-- clear keeps all of them and only empties the values. Every scenario runs twice, with table.clear and with a reference
-- implementation in Lua (`for k in pairs(t) do t[k] = nil end`), and the results must agree. Run under
-- RUSTYNAIL_JIT=always / RUSTYNAIL_JIT_TRACE=always / RUSTYNAIL_GC_STRESS to exercise the compiled paths.
local tclear = require("table.clear")
local function ref_clear(t) for k in pairs(t) do t[k] = nil end end

local function scenario_array(clear)
  local t = table.new(32, 0)
  local acc = 0
  for round = 1, 400 do
    for i = 1, 32 do t[i] = i * round end
    for i = 1, 32 do acc = acc + t[i] end
    clear(t)
    acc = acc + (t[5] or 7) + #t
  end
  return acc
end

local function scenario_hash(clear)
  local t = table.new(0, 8)
  local acc = 0
  for round = 1, 400 do
    t.a = round; t.b = round * 2; t.c = round * 3; t.d = round * 4
    acc = acc + t.a + t.b + t.c + t.d
    clear(t)
    acc = acc + (t.a or 1) + (t.d or 2)
  end
  return acc
end

local function scenario_mixed(clear)
  local t = { 1, 2, 3, x = 1, y = 2 }
  local acc = 0
  for round = 1, 400 do
    t[1] = round; t[2] = round + 1; t.x = round; t.z = round
    acc = acc + t[1] + t[2] + t.x + t.z
    if round % 3 == 0 then clear(t) end
    acc = acc + (t[1] or 0) + (t.x or 0)
  end
  return acc
end

-- the same table read by a loop that is hot (trace) while another call clears it from time to time
local function scenario_reader(clear)
  local t = { 10, 20, 30, 40, 50, k = 5 }
  local acc = 0
  for i = 1, 3000 do
    acc = acc + (t[(i % 5) + 1] or -1) + (t.k or -2)
    if i % 97 == 0 then clear(t) end
    if i % 101 == 0 then t[1], t[2], t[3], t[4], t[5], t.k = 10, 20, 30, 40, 50, 5 end
  end
  return acc
end

-- table.clear called from inside a metamethod and a coroutine while loops are hot
local function scenario_mm_coro(clear)
  local t = { 1, 2, 3 }
  local mt = { __index = function(self, k) clear(self); return 0 end }
  local u = setmetatable({}, mt)
  local acc = 0
  local co = coroutine.wrap(function()
    for i = 1, 600 do
      t[1], t[2], t[3] = i, i, i
      acc = acc + t[2]
      clear(t)
      if i % 50 == 0 then coroutine.yield(acc) end
    end
    return acc
  end)
  for i = 1, 13 do acc = acc + (co() or 0) end
  for i = 1, 500 do u[1] = nil; acc = acc + u[i] end
  return acc
end

-- clear on tables that hold collectable values, with the collector running
local function scenario_gc(clear)
  local t = table.new(8, 8)
  local n = 0
  for round = 1, 300 do
    for i = 1, 8 do t[i] = { round }; t["k" .. i] = { round } end
    clear(t)
    if round % 25 == 0 then collectgarbage() end
    n = n + #t
  end
  for i = 1, 8 do t[i] = { 1 }; t["k" .. i] = { i } end
  collectgarbage()
  return n + #t + t.k8[1]
end

local bad = 0
for name, f in pairs({ array = scenario_array, hash = scenario_hash, mixed = scenario_mixed, reader = scenario_reader,
                       mm_coro = scenario_mm_coro, gc = scenario_gc }) do
  local a, b = f(tclear), f(ref_clear)
  if a ~= b then bad = bad + 1; print("MISMATCH", name, a, b) end
end
-- repeat so that compiled code / traces exist for the second round
for rep = 1, 3 do
  for _, f in ipairs({ scenario_array, scenario_hash, scenario_mixed, scenario_reader }) do
    if f(tclear) ~= f(ref_clear) then bad = bad + 1; print("MISMATCH rep", rep) end
  end
end
print("clear-jit", bad == 0 and "ok" or "FAILED")
