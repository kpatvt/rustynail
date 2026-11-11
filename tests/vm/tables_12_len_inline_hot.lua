-- `#t` in hot loops (tier-1 LEN and trace TLEN take an inline path on the common array states): the answers (and the state the
-- length hint leaves behind) must be those of the plain helper. Border cases: stack push/pop, holes, removal, hash-part integers,
-- sizes at array boundaries, __len, non-table operands, tables without array part.
local out = {}
local function emit(x) out[#out + 1] = tostring(x) end

-- stack push / pop (hint follows the top)
local st = {}
local acc = 0
for r = 1, 400 do
  for i = 1, 20 do st[#st + 1] = i end
  acc = acc + #st
  for i = 1, 20 do st[#st] = nil acc = acc + #st end
end
emit(acc) emit(#st)

-- fill past array sizes 1,2,4,8,...: #t checked at every size
local t = {}
local s = 0
for i = 1, 300 do t[#t + 1] = i s = s + #t end
emit(s)
for i = 300, 1, -1 do s = s + #t t[i] = nil end
emit(s) emit(#t)

-- queue-like: head moves, tail grows, holes at the front never matter for a border at the end
local q, head = {}, 1
local c = 0
for i = 1, 1000 do
  q[#q + 1] = i
  if i % 3 == 0 then q[head] = nil head = head + 1 end
  c = c + (#q >= 1 and 1 or 0)
end
emit(c) emit(head)

-- hash-part integers beyond the array
local h = {}
for i = 1, 6 do h[i] = i end
h[8] = 8 h[9] = 9
emit(#h)
h[7] = 7
emit(#h)
local hh = {10, 20, 30, [4] = 40, [5] = 50}
emit(#hh)
local only_hash = {}
only_hash[1] = 1 only_hash[2] = 2 only_hash[3] = 3
emit(#only_hash)

-- exactly at power-of-two boundaries and full arrays (hint == asize)
for _, n in ipairs{1, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65} do
  local a = {}
  for i = 1, n do a[i] = true end
  local sum = 0
  for r = 1, 60 do sum = sum + #a end
  local b = table.pack(table.unpack(a, 1, n))
  local sb = 0
  for r = 1, 60 do sb = sb + #b end
  emit(n .. ":" .. sum .. ":" .. sb)
  a[#a + 1] = true
  a.x = 1
  emit(#a)
end

-- array with the hint on a hole: border from either side (must equal the helper's)
local ho = {}
for i = 1, 16 do ho[i] = i end
local res = {}
for k = 1, 16 do
  ho[k] = nil
  local n = 0
  for r = 1, 40 do n = n + #ho end
  res[#res + 1] = #ho
  ho[k] = k
end
emit(table.concat(res, ","))
local ho2 = {}
for i = 1, 16 do ho2[i] = i end
for k = 16, 1, -1 do
  ho2[k] = nil
  local n = 0
  for r = 1, 40 do n = n + #ho2 end
  emit(#ho2)
end

-- tables with no array part, with a hash part, empty
local e = {}
local z = 0
for r = 1, 200 do z = z + #e end
emit(z)
local ek = {a = 1, b = 2}
for r = 1, 200 do z = z + #ek end
emit(z)
ek[1] = 1
emit(#ek)

-- __len and strings / mixed operands in the same loop
local mt = setmetatable({1, 2, 3}, {__len = function() return 42 end})
local plain = {1, 2, 3}
local withmt = setmetatable({1, 2, 3, 4}, {__index = function() return 0 end})
local str = "hello"
local objs = {plain, mt, withmt, str, plain, {}, {1}, mt}
local tot = 0
for r = 1, 300 do
  for k = 1, #objs do tot = tot + #objs[k] end
end
emit(tot)
do local ok, err = pcall(function() local v = 5; for r = 1, 100 do local _ = #v end end) print(ok, (err:gsub("^.-:%d+: ", ""))) end

-- length of a table being grown inside a trace loop (TLEN feeds the key of TSET)
local g = {}
for i = 1, 1000 do g[#g + 1] = i * 2 end
emit(#g) emit(g[#g])
local g2 = {}
for i = 1, 700 do g2[#g2 + 1] = i if i % 7 == 0 then g2[#g2] = nil end end
emit(#g2)

-- random sets / clears / pushes / pops of a 32-slot array with holes (no key beyond slot 32, so the hash part stays empty and every
-- border is defined by the length hint alone), `#` after every step
do
  local rseed = 12345
  local function rnd(n) rseed = (rseed * 1103515245 + 12345) % 2147483648 return rseed % n + 1 end
  local function fill32() return { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32 } end
  local rt, rsum = fill32(), 0
  for step = 1, 20000 do
    local op = rnd(9)
    if op <= 2 then if #rt < 32 then rt[#rt + 1] = step end
    elseif op <= 4 then local n = #rt if n > 0 then rt[n] = nil end
    elseif op == 5 then rt[rnd(32)] = step
    elseif op == 6 then rt[rnd(32)] = nil
    elseif op == 7 then rt[rnd(6)] = nil
    elseif op == 8 then local n = #rt if n < 31 then rt[n + 1] = step rt[n + 2] = step end
    else local c0 = rnd(26) for k = 0, 5 do rt[c0 + k] = nil end end
    rsum = rsum + #rt * (step % 7 + 1)
    if step % 5000 == 0 then rt = fill32() end
  end
  emit(rsum) emit(#rt)
end

-- fresh constructor tables (length hint 0): all set, a nil at each position, sizes around the five-slot limit of the helper's upward search
do
  local zs = 0
  for i = 1, 400 do
    local a = { i }
    local b = { i, i }
    local c = { i, i, nil }
    local d = { i, nil, i }
    local e = { nil, i }
    local f = { i, i, i, i, i }
    local g = { i, i, i, i, i, i }
    local h = { i, i, i, i, i, i, nil, i }
    local k = { i, i, i, i, nil, i }
    local m = { i, i, i, i, i, nil }
    local n13 = { i, i, i, i, i, i, i, i, i, i, i, i, i, nil, i, i }
    local n7 = { i, i, i, i, i, i, nil, i, i }
    zs = zs + #n13 * 31 + #n7 * 37
    zs = zs + #a + #b * 3 + #c * 5 + #d * 7 + #e * 11 + #f * 13 + #g * 17 + #h * 19 + #k * 23 + #m * 29
    b[3] = i
    zs = zs + #b + #f
  end
  emit(zs)
end

for i = 1, #out do print(out[i]) end
