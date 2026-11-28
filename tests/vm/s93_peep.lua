-- Section 93.1 (`peep`): fused field compares (the node-vector size and the array size of a table read in memory), `TBAR` colour tests and its
-- out-of-line slow path, `HMISS` without the key-tag compare. Cases: barriers of fresh objects stored into an old table under incremental and
-- generational collection, a table whose hash part grows / shrinks while a loop reads its fields, method lookups through `__index` with a receiver
-- that gets the method key (and keys of other types) later, integer / float / boolean keys next to the string keys an `HMISS` tests.

local function sum(t)
  local s = 0
  for _, v in ipairs(t) do s = s + v.n end
  return s
end

-- (1) barriers: an old table gets fresh tables / strings / closures in a loop, collections in between
for _, mode in ipairs({ "incremental", "generational" }) do
  collectgarbage(mode)
  local keep = {}
  for i = 1, 200 do keep[i] = { n = 0 } end
  collectgarbage()
  collectgarbage()
  for round = 1, 30 do
    for i = 1, 200 do
      keep[i] = { n = i + round }
      keep[i].s = "s" .. i .. "_" .. round
      keep[i].f = function() return i * round end
      if i % 50 == 0 then collectgarbage("step", 1) end
    end
    if round % 7 == 0 then collectgarbage() end
  end
  local ok = 0
  for i = 1, 200 do
    if keep[i].s == "s" .. i .. "_30" and keep[i].f() == i * 30 then ok = ok + 1 end
  end
  print("1", mode, sum(keep), ok)
end
collectgarbage("incremental")

-- (1b) the barrier into a hash field of an old object and into an upvalue-held table
do
  local holder = { slot = false, list = {} }
  collectgarbage()
  local acc = 0
  for i = 1, 3000 do
    holder.slot = { v = i }
    holder.list[(i % 17) + 1] = { v = -i }
    if i % 100 == 0 then collectgarbage("step", 0) end
    acc = acc + holder.slot.v + holder.list[(i % 17) + 1].v
  end
  collectgarbage()
  print("1b", acc, holder.slot.v, holder.list[1].v)
end

-- (2) a table whose node vector grows and shrinks while a loop reads its fields
do
  local t = { a = 1, b = 2 }
  local s = 0
  for i = 1, 4000 do
    s = s + t.a + t.b
    if i == 1000 then for k = 1, 20 do t["k" .. k] = k end end
    if i == 2000 then for k = 1, 20 do t["k" .. k] = nil end; t.a = 10; collectgarbage() end
    if i == 2500 then t = { b = 5, a = 7 } end
    if i == 3000 then t = setmetatable({ b = 1 }, { __index = { a = 100 } }) end
  end
  print("2", s)
end

-- (2b) the array size compared: a loop over a table whose array part changes size
do
  local t = { 1, 2, 3 }
  local s = 0
  for i = 1, 3000 do
    s = s + (t[1] or 0) + (t[3] or 0)
    if i == 1000 then t[4] = 4; t[5] = 5 end
    if i == 2000 then t = { 9 } end
  end
  print("2b", s)
end

-- (3) method lookups through `__index`; the receiver gets the method key later, keys of other types beside it
do
  local C = {}
  C.__index = C
  function C:get() return self.v end
  function C:inc(d) self.v = self.v + d end
  local objs = {}
  for i = 1, 8 do objs[i] = setmetatable({ v = i }, C) end
  objs[3][1] = true
  objs[4][2.5] = "x"
  objs[5][false] = 1
  local s = 0
  for r = 1, 600 do
    for i = 1, 8 do
      local o = objs[i]
      o:inc(1)
      s = s + o:get()
    end
    if r == 300 then
      objs[2].get = function(self) return -self.v end
      rawset(objs[6], "inc", function(self, d) self.v = self.v + 2 * d end)
    end
    if r == 450 then objs[2].get = nil end
  end
  print("3", s)
end

-- (4) many string keys in one node vector (the `HMISS` nodes collide), a key added to the receiver mid-run
do
  local B = { m = function(self) return self.x end }
  B.__index = B
  local o = setmetatable({ x = 1 }, B)
  for k = 1, 12 do o["f" .. k] = k end
  local s = 0
  for i = 1, 3000 do
    s = s + o:m()
    if i == 1500 then o.m = function() return 1000 end end
    if i == 2200 then o.m = nil end
  end
  print("4", s)
end
