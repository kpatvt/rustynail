-- Section 93.2 (`smallkeys`): the absent-key proof of a method key in a receiver with a small node vector is the key of every node. Cases: a
-- deleted field whose node is reused by the method key (for several field-name pairs, so that the method key's main position meets the
-- deleted field's node), a receiver that grows (rehash), a receiver with one node, a receiver whose node holds a key of another type, a field
-- set to nil and back, many receivers of one class with different field orders, the class method replaced.

local C = {}
C.__index = C
function C:m() return 1 end
function C:get() return self.a end

local names = { "a", "b", "c", "d", "e", "f", "g", "h", "x1", "x2", "y1", "y2", "zz", "qq", "pp", "rr" }

-- (1) the method key takes the node of a deleted field
local total = 0
for p = 1, #names - 1 do
  local f1, f2 = names[p], names[p + 1]
  local o = setmetatable({ [f1] = 1, [f2] = 2 }, C)
  local s = 0
  for i = 1, 400 do
    s = s + o:m()
    if i == 200 then
      o[f1] = nil
      o.m = function() return 100 end
    end
  end
  total = total + s
  print("1", f1, f2, s)
end
print("1 total", total)

-- (2) a receiver that grows, a receiver with one node, a key of another type in a node
do
  local o = setmetatable({ a = 1, b = 2 }, C)
  local p = setmetatable({ a = 5 }, C)
  local q = setmetatable({ a = 7, [3.5] = true }, C)
  local s = 0
  for i = 1, 600 do
    s = s + o:m() + o:get() + p:m() + p:get() + q:m() + q:get()
    if i == 200 then o.c = 3 end
    if i == 300 then p.m = function() return 50 end end
    if i == 400 then q[3.5] = nil; q.m = function() return 9 end end
  end
  print("2", s)
end

-- (3) a field set to nil and back, the class method replaced
do
  local o = setmetatable({ a = 1, b = 2 }, C)
  local s = 0
  for i = 1, 600 do
    s = s + o:m()
    if i == 100 then o.a = nil end
    if i == 150 then o.a = 4 end
    if i == 300 then C.m = function() return 3 end end
    if i == 450 then o.b = nil; o.b = 8 end
  end
  print("3", s, o.a, o.b)
  C.m = function() return 1 end
end

-- (4) many receivers, different field orders and node layouts
do
  local objs = {}
  for i = 1, 12 do
    if i % 3 == 0 then objs[i] = setmetatable({ b = i, a = -i }, C)
    elseif i % 3 == 1 then objs[i] = setmetatable({ a = i }, C)
    else objs[i] = setmetatable({ a = i, zz = 0 }, C) end
  end
  local s = 0
  for r = 1, 200 do
    for i = 1, 12 do s = s + objs[i]:m() + objs[i]:get() end
    if r == 120 then objs[5].m = function() return 1000 end end
  end
  print("4", s)
end
