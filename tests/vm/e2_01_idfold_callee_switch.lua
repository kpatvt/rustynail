-- E2 (region-scoped callee identity, docs/JIT_TRACE.md section 34): a self-recursive function whose callee is its own upvalue is called
-- through that upvalue without the identity chain inside a region; every way the upvalue (or the function value) can change must be seen
-- exactly as the interpreter sees it.

local function uvidx(fn, name)
  for i = 1, 20 do local n = debug.getupvalue(fn, i) if n == name then return i end if n == nil then return nil end end
end

-- (1) the upvalue reassigned by the main chunk between two recursions (outer entries check again)
local f
f = function(n) if n < 2 then return n end return f(n - 1) + f(n - 2) end
local f0 = f
local s1 = 0
for i = 1, 120 do
  if i == 60 then f = function(n) return 1000 + f0(n) end end
  s1 = s1 + f0(i % 12)
end
print("reassign", s1)
f = f0

-- (2) the upvalue replaced from a leaf, through a C function (debug.setupvalue), while frames of the old region are suspended above
-- the root: the remaining second calls of the suspended frames call the new function
local g
local alt = function(n) return 7 end
local flip = { n = 0 }
g = function(n)
  if n < 2 then
    flip.n = flip.n + 1
    if flip.n == 50 then debug.setupvalue(g, uvidx(g, "g"), alt) end
    return n
  end
  return g(n - 1) + g(n - 2)
end
local g0 = g
local s2 = 0
for i = 1, 40 do s2 = s2 + g0(i % 10) end
print("leaf setupvalue", s2, g == alt)

-- (3) debug.upvaluejoin: the self upvalue made to share another closure's upvalue cell
local h
h = function(n) if n == 0 then return 0 end return 1 + h(n - 1) end
for i = 1, 50 do h(i % 30) end
local other = h
local box = function(n) return -100 end
local function holder() return box end
debug.upvaluejoin(h, uvidx(h, "h"), holder, uvidx(holder, "box"))
print("upvaluejoin", other(10), other(0))

-- (4) a global recursion replaced (the callee is not an upvalue: never folded) next to a local one
function gfib(n) if n < 2 then return n end return gfib(n - 1) + gfib(n - 2) end
local s4 = 0
for i = 1, 80 do
  if i == 40 then local old = gfib gfib = function(n) return old(n) * 2 end end
  s4 = s4 + gfib(i % 11)
end
print("global", s4)

-- (5) many closures of one prototype (each reaches itself through its own upvalue), alternating between outer entries
local function make(add)
  local r
  r = function(n) if n == 0 then return add end return r(n - 1) + 1 end
  return r
end
local rs = {}
for k = 1, 5 do rs[k] = make(k * 10) end
local s5 = 0
for i = 1, 400 do s5 = s5 + rs[i % 5 + 1](i % 17) end
print("closures", s5)

-- (6) one closure's self upvalue pointing to another closure of the same prototype (copied upvalue): identity differs from the running closure
local a1 = make(1)
local a2 = make(2)
debug.upvaluejoin(a1, uvidx(a1, "r"), a2, uvidx(a2, "r"))   -- a1 now calls a2 for its recursion
local s6 = 0
for i = 1, 100 do s6 = s6 + a1(i % 9) + a2(i % 9) end
print("crossed", s6)

-- (7) a callable table (__call) in the recursion, the metamethod replaced mid-run
local mt = {}
local obj = setmetatable({}, mt)
mt.__call = function(self, n) if n == 0 then return 0 end return obj(n - 1) + 1 end
local s7 = 0
for i = 1, 100 do
  if i == 50 then mt.__call = function(self, n) if n == 0 then return 5 end return obj(n - 1) + 2 end end
  s7 = s7 + obj(i % 12)
end
print("__call", s7)

-- (8) method recursion whose __index changes mid-run
local A = { depth = function(self, n) if n == 0 then return 0 end return self:depth(n - 1) + 1 end }
local B = { depth = function(self, n) if n == 0 then return 100 end return self:depth(n - 1) + 3 end }
local inst = setmetatable({}, { __index = A })
local s8 = 0
for i = 1, 100 do
  if i == 50 then setmetatable(inst, { __index = B }) end
  s8 = s8 + inst:depth(i % 15)
end
print("__index", s8)

-- (9) an upvalue store inside the recursive family (an inlined callee assigns another upvalue): the family rule
local counter = 0
local function bump() counter = counter + 1 return 0 end
local rec
-- (every CALL of `rec` wants one result: the prototype is lazy-aware, the family under the region invariant)
rec = function(n) if n == 0 then return 0 end local b = bump() return rec(n - 1) + 1 + b end
local s9 = 0
for i = 1, 300 do s9 = s9 + rec(i % 20) end
print("ustore", s9, counter)

-- (10) the self upvalue assigned by an inlined callee inside the recursion (the store hits the self upvalue itself)
local sw
local function swapper() sw = function(n) return -1 end return 0 end
sw = function(n) local z = 0 if n == 3 then z = swapper() end if n == 0 then return 0 end return sw(n - 1) + 1 + z end
local sw0 = sw
local s10 = 0
for i = 1, 60 do s10 = s10 + sw0(i % 8) sw = sw0 end
print("self store", s10)

-- (11) a self-call through a table field (not folded) in a family under the region invariant: after the field is switched to another closure
-- of the prototype whose self upvalue names the first closure, the natively pushed frame must not be entered with a prototype guard (its
-- folded `GETUPVAL` would call itself instead of the closure its upvalue names)
local function make2(k)
  local r
  r = function(n, t) if n == 0 then return 0 end if n % 3 == 0 then return t.f(n - 1, t) + k end return r(n - 1, t) + k end
  return r
end
local r1, r2 = make2(1), make2(2)
debug.upvaluejoin(r2, uvidx(r2, "r"), r1, uvidx(r1, "r"))
local tf = { f = r1 }
local s11 = 0
for i = 1, 300 do
  if i == 150 then tf.f = r2 end
  s11 = s11 + r1(i % 25, tf)
end
print("field switch", s11)

-- (12) the self upvalue assigned by an inlined callee on a hot path in the middle of the recursion, after the frame's first self-call:
-- the second self-calls of the frames below it must call the new function (the family rule refuses the trace with the upvalue store;
-- without it a folded `GETUPVAL` of a continuation trace in the same region would still call the old closure)
local fsw
local alt2 = function(n) return 1000 end
local on = { v = false }
local function swap2() fsw = alt2 return 0 end
fsw = function(n)
  if n < 2 then return n end
  local a = fsw(n - 1)
  local z = 0
  if n == 5 and on.v then on.v = false z = swap2() end
  return a + fsw(n - 2) + z
end
local fsw0 = fsw
local s12 = 0
for i = 1, 300 do
  on.v = i >= 100
  s12 = s12 + fsw0(i % 4 + 9)
  fsw = fsw0
end
print("mid store", s12)

-- (13) crossed self upvalues of a tree recursion: the region root (a frame of `b2`) returns into a frame of `b1`, whose self upvalue names `b2`;
-- the caller's continuation makes its second self-call next, which must call `b2` (the root return checks the new root again)
local function make3(k)
  local r
  r = function(n) if n < 2 then return n * k end return r(n - 1) + r(n - 2) end
  return r
end
local b1, b2 = make3(1), make3(3)
local s13 = 0
for i = 1, 200 do s13 = s13 + b1(i % 12) + b2(i % 12) end
debug.upvaluejoin(b1, uvidx(b1, "r"), b2, uvidx(b2, "r"))
for i = 1, 200 do s13 = s13 + b1(i % 12) + b2(i % 12) end
print("crossed tree", s13)
